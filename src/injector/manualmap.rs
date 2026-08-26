use super::pe_util::PeParser;
use super::{InjectMethod, InjectResult};
use crate::error::InjectorError;
use std::fs;
use std::path::Path;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, VirtualProtectEx,
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE,
    PAGE_EXECUTE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE,
    PAGE_NOACCESS, PAGE_PROTECTION_FLAGS, PAGE_READONLY, PAGE_READWRITE,
};
use windows::Win32::System::Threading::{
    CreateRemoteThread, GetExitCodeThread, OpenProcess, TerminateThread, WaitForSingleObject,
    PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
};

fn build_dllmain_stub(remote_base: usize, entry: usize) -> Vec<u8> {
    let mut c = Vec::with_capacity(32);
    c.push(0x48); c.push(0xB9);
    c.extend_from_slice(&remote_base.to_le_bytes());
    c.push(0xBA); c.push(0x01); c.push(0x00); c.push(0x00); c.push(0x00);
    c.push(0x45); c.push(0x31); c.push(0xC0);
    c.push(0x48); c.push(0xB8);
    c.extend_from_slice(&entry.to_le_bytes());
    c.push(0xFF); c.push(0xD0);
    c.push(0x48); c.push(0x31); c.push(0xC0);
    c.push(0xC3);
    c
}

fn prot_from_chars(chars: u32) -> PAGE_PROTECTION_FLAGS {
    let executable = chars & 0x20000000 != 0;
    let readable   = chars & 0x40000000 != 0;
    let writable   = chars & 0x80000000 != 0;
    match (executable, readable, writable) {
        (true,  true,  true)  => PAGE_EXECUTE_READWRITE,
        (true,  true,  false) => PAGE_EXECUTE_READ,
        (true,  false, true)  => PAGE_EXECUTE_READWRITE,
        (true,  false, false) => PAGE_EXECUTE,
        (false, true,  true)  => PAGE_READWRITE,
        (false, true,  false) => PAGE_READONLY,
        (false, false, true)  => PAGE_READWRITE,
        (false, false, false) => PAGE_NOACCESS,
    }
}

pub fn inject(pid: u32, dll_path: &Path) -> Result<InjectResult, InjectorError> {
    crate::logger::info(&format!("[ManualMap] {}", dll_path.display()));

    let data = fs::read(dll_path).map_err(|e| InjectorError::PeParseFailed(e.to_string()))?;
    let pe = PeParser::new(data.clone()).map_err(|e| InjectorError::PeParseFailed(e.to_string()))?;
    if !pe.is_64bit() {
        return Err(InjectorError::ArchMismatch("Not 64-bit".into()));
    }

    let handle: HANDLE = unsafe {
        OpenProcess(
            PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ,
            false, pid,
        )
    }.map_err(|e| InjectorError::ProcessNotFound(format!("OpenProcess: {:?}", e)))?;

    let size = pe.size_of_image();
    let base = unsafe {
        VirtualAllocEx(handle, None, size, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE)
    };
    if base.is_null() {
        let _ = unsafe { CloseHandle(handle) };
        return Err(InjectorError::MemoryAllocFailed(0));
    }

    // Map headers
    let mut w = 0usize;
    unsafe {
        WriteProcessMemory(handle, base, data.as_ptr() as *const std::ffi::c_void, pe.size_of_headers(), Some(&mut w))
            .map_err(|e| InjectorError::CommunicationError(format!("Write header: {:?}", e)))?;
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
                .map_err(|e| InjectorError::CommunicationError(format!("Write section: {:?}", e)))?;
        }
    }

    // Fix relocations
    let delta = base as isize - pe.image_base() as isize;
    pe.apply_relocations_remote(handle, base, delta)
        .map_err(|e| InjectorError::PeParseFailed(e))?;

    // Fill imports using fixed method
    pe.fill_imports_remote_fixed(pid, handle, base)
        .map_err(|e| InjectorError::PeParseFailed(e))?;

    // Set correct section protections
    for sec in pe.sections() {
        let addr = (base as usize + sec.VirtualAddress as usize) as *mut std::ffi::c_void;
        let prot = prot_from_chars(sec.Characteristics);
        let mut old = PAGE_PROTECTION_FLAGS(0);
        unsafe {
            VirtualProtectEx(handle, addr, sec.VirtualSize as usize, prot, &mut old)
                .map_err(|e| InjectorError::CommunicationError(format!("VirtualProtectEx: {:?}", e)))?;
        }
    }

    // Build and write shellcode stub
    let entry = base as usize + pe.entry_point_rva();
    let stub = build_dllmain_stub(base as usize, entry);
    let stub_mem = unsafe {
        VirtualAllocEx(handle, None, stub.len(), MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE)
    };
    if stub_mem.is_null() {
        let _ = unsafe { VirtualFreeEx(handle, base, 0, MEM_RELEASE); CloseHandle(handle); };
        return Err(InjectorError::MemoryAllocFailed(0));
    }
    unsafe {
        WriteProcessMemory(handle, stub_mem, stub.as_ptr() as *const std::ffi::c_void, stub.len(), Some(&mut w))
            .map_err(|e| InjectorError::CommunicationError(format!("Write stub: {:?}", e)))?;
    }

    let thread = unsafe {
        CreateRemoteThread(handle, None, 0, Some(std::mem::transmute(stub_mem)), None, 0, None)
    };

    match thread {
        Ok(th) => {
            let wait = unsafe { WaitForSingleObject(th, 30000) };
            if wait == windows::Win32::Foundation::WAIT_TIMEOUT {
                let _ = unsafe { TerminateThread(th, 1); CloseHandle(th); VirtualFreeEx(handle, stub_mem, 0, MEM_RELEASE); VirtualFreeEx(handle, base, 0, MEM_RELEASE); CloseHandle(handle); };
                return Err(InjectorError::Timeout);
            }
            let mut code: u32 = 0;
            unsafe { GetExitCodeThread(th, &mut code).ok(); }
            let _ = unsafe { CloseHandle(th); VirtualFreeEx(handle, stub_mem, 0, MEM_RELEASE); CloseHandle(handle); };
            Ok(InjectResult { method: InjectMethod::ManualMap, base_address: base as usize, dll_path: dll_path.display().to_string() })
        }
        Err(e) => {
            let _ = unsafe { VirtualFreeEx(handle, stub_mem, 0, MEM_RELEASE); VirtualFreeEx(handle, base, 0, MEM_RELEASE); CloseHandle(handle); };
            Err(InjectorError::CommunicationError(format!("CreateRemoteThread: {:?}", e)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMAGE_SCN_MEM_EXECUTE: u32 = 0x20000000;
    const IMAGE_SCN_MEM_READ: u32 = 0x40000000;
    const IMAGE_SCN_MEM_WRITE: u32 = 0x80000000;

    #[test]
    fn prot_exec_read_write() {
        let chars = IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_WRITE;
        assert_eq!(prot_from_chars(chars).0, PAGE_EXECUTE_READWRITE.0);
    }

    #[test]
    fn prot_exec_read_only() {
        let chars = IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ;
        assert_eq!(prot_from_chars(chars).0, PAGE_EXECUTE_READ.0);
    }

    #[test]
    fn prot_exec_only() {
        let chars = IMAGE_SCN_MEM_EXECUTE;
        assert_eq!(prot_from_chars(chars).0, PAGE_EXECUTE.0);
    }

    #[test]
    fn prot_read_write() {
        let chars = IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_WRITE;
        assert_eq!(prot_from_chars(chars).0, PAGE_READWRITE.0);
    }

    #[test]
    fn prot_read_only() {
        let chars = IMAGE_SCN_MEM_READ;
        assert_eq!(prot_from_chars(chars).0, PAGE_READONLY.0);
    }

    #[test]
    fn prot_no_access() {
        assert_eq!(prot_from_chars(0).0, PAGE_NOACCESS.0);
    }

    #[test]
    fn prot_write_alone_maps_to_rw() {
        // 仅 WRITE（可读位缺失）按 PAGE_READWRITE 处理，避免写入后不可读
        let chars = IMAGE_SCN_MEM_WRITE;
        assert_eq!(prot_from_chars(chars).0, PAGE_READWRITE.0);
    }
}