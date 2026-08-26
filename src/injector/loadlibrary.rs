use super::{InjectMethod, InjectResult};
use crate::error::InjectorError;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Memory::{VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE};
use windows::Win32::System::Threading::{
    CreateRemoteThread, GetExitCodeThread, OpenProcess, TerminateThread, WaitForSingleObject,
    PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
};
use windows::core::{PCSTR, PCWSTR};

pub fn inject(pid: u32, dll_path: &Path) -> Result<InjectResult, InjectorError> {
    crate::logger::info(&format!("[LoadLibrary] {}", dll_path.display()));

    let handle = unsafe {
        OpenProcess(
            PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ,
            false, pid,
        )
    }.map_err(|e| InjectorError::ProcessNotFound(format!("OpenProcess: {:?}", e)))?;

    let path_str = dll_path.to_str().ok_or(InjectorError::Unknown("Invalid path".into()))?;
    let wide: Vec<u16> = OsStr::new(path_str).encode_wide().chain(Some(0)).collect();
    let size = wide.len() * 2;

    let remote = unsafe {
        VirtualAllocEx(handle, None, size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE)
    };
    if remote.is_null() {
        let _ = unsafe { CloseHandle(handle) };
        return Err(InjectorError::MemoryAllocFailed(0));
    }

    let mut written = 0usize;
    unsafe {
        WriteProcessMemory(handle, remote, wide.as_ptr() as *const std::ffi::c_void, size, Some(&mut written))
            .map_err(|e| {
                let _ = unsafe { VirtualFreeEx(handle, remote, 0, MEM_RELEASE); CloseHandle(handle); };
                InjectorError::CommunicationError(format!("WriteProcessMemory: {:?}", e))
            })?;
    }

    let k32: Vec<u16> = "kernel32.dll\0".encode_utf16().collect();
    let hmod = unsafe { GetModuleHandleW(PCWSTR(k32.as_ptr())) }
        .map_err(|e| InjectorError::CommunicationError(format!("GetModuleHandleW: {:?}", e)))?;
    let addr = unsafe { GetProcAddress(hmod, PCSTR("LoadLibraryW\0".as_ptr())) }
        .ok_or(InjectorError::ExportNotFound("LoadLibraryW".into()))? as *const std::ffi::c_void;

    let thread = unsafe {
        CreateRemoteThread(
            handle, None, 0,
            Some(std::mem::transmute(addr)),
            Some(remote), 0, None,
        )
    };

    let th = match thread {
        Ok(h) => h,
        Err(e) => {
            let _ = unsafe { VirtualFreeEx(handle, remote, 0, MEM_RELEASE); CloseHandle(handle); };
            return Err(InjectorError::CommunicationError(format!("CreateRemoteThread: {:?}", e)));
        }
    };

    let wait = unsafe { WaitForSingleObject(th, 30000) };
    if wait == WAIT_TIMEOUT {
        let _ = unsafe { TerminateThread(th, 1); CloseHandle(th); VirtualFreeEx(handle, remote, 0, MEM_RELEASE); CloseHandle(handle); };
        return Err(InjectorError::Timeout);
    }

    let mut code: u32 = 0;
    unsafe { GetExitCodeThread(th, &mut code).map_err(|e| InjectorError::CommunicationError(format!("GetExitCodeThread: {:?}", e)))?; }
    let _ = unsafe { CloseHandle(th); VirtualFreeEx(handle, remote, 0, MEM_RELEASE); CloseHandle(handle); };

    if code == 0 {
        Err(InjectorError::DllLoadFailed("LoadLibraryW returned NULL (arch mismatch or missing deps)".into()))
    } else {
        Ok(InjectResult { method: InjectMethod::LoadLibrary, base_address: code as usize, dll_path: path_str.to_string() })
    }
}