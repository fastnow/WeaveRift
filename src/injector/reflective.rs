use super::pe_util::PeParser;
use super::{InjectMethod, InjectResult};
use std::fs;
use std::path::Path;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows::Win32::System::Memory::{VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READWRITE};
use windows::Win32::System::Threading::{
    CreateRemoteThread, GetExitCodeThread, OpenProcess, WaitForSingleObject,
    PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
};

pub fn inject(pid: u32, dll_path: &Path) -> Result<InjectResult, String> {
    crate::logger::info(&format!("[Reflective] {}", dll_path.display()));

    let data = fs::read(dll_path).map_err(|e| e.to_string())?;
    let pe = PeParser::new(data.clone()).map_err(|e| e.to_string())?;
    if !pe.is_64bit() { return Err("Not 64-bit".into()); }

    let handle: HANDLE = unsafe {
        OpenProcess(
            PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ,
            false, pid,
        )
    }.map_err(|e| format!("OpenProcess: {:?}", e))?;

    let size = pe.size_of_image();
    let base = unsafe {
        VirtualAllocEx(handle, None, size, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE)
    };
    if base.is_null() {
        let _ = unsafe { CloseHandle(handle) };
        return Err("VirtualAllocEx failed".into());
    }

    let delta = base as isize - pe.image_base() as isize;
    if delta != 0 {
        crate::logger::info(&format!("[Reflective] Base mismatch, applying relocations (delta: 0x{:X})", delta));
        if let Err(e) = pe.apply_relocations_remote(handle, base, delta) {
            let _ = unsafe { VirtualFreeEx(handle, base, 0, MEM_RELEASE); CloseHandle(handle); };
            return Err(format!("Relocation failed: {}", e));
        }
    }

    // Map headers
    let mut w = 0usize;
    unsafe {
        WriteProcessMemory(handle, base, data.as_ptr() as *const std::ffi::c_void, pe.size_of_headers(), Some(&mut w))
            .map_err(|e| format!("Write header: {:?}", e))?;
    }

    // Map sections
    for sec in pe.sections() {
        let raw = sec.SizeOfRawData as usize;
        if raw == 0 { continue; }
        let off = sec.PointerToRawData as usize;
        let va = sec.VirtualAddress as usize;
        let addr = (base as usize + va) as *mut std::ffi::c_void;
        unsafe {
            WriteProcessMemory(handle, addr, data.as_ptr().add(off) as *const std::ffi::c_void, raw, Some(&mut w))
                .map_err(|e| format!("Write section: {:?}", e))?;
        }
    }

    let rva = pe.get_export_rva("ReflectiveLoader")
        .ok_or("ReflectiveLoader export not found")?;
    let entry = (base as usize + rva) as *const std::ffi::c_void;

    let thread = unsafe {
        CreateRemoteThread(
            handle, None, 0,
            Some(std::mem::transmute::<*const std::ffi::c_void, extern "system" fn(*mut std::ffi::c_void) -> u32>(entry)),
            Some(base), 0, None,
        )
    };

    match thread {
        Ok(th) => {
            unsafe { WaitForSingleObject(th, 30000) };
            let mut code: u32 = 0;
            unsafe { GetExitCodeThread(th, &mut code).ok(); }
            let _ = unsafe { CloseHandle(th); CloseHandle(handle); };
            Ok(InjectResult { method: InjectMethod::Reflective, base_address: base as usize, dll_path: dll_path.display().to_string() })
        }
        Err(e) => {
            let _ = unsafe { VirtualFreeEx(handle, base, 0, MEM_RELEASE); CloseHandle(handle); };
            Err(format!("CreateRemoteThread: {:?}", e))
        }
    }
}