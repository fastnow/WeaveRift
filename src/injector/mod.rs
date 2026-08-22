pub mod loadlibrary;
pub mod reflective;
pub mod manualmap;
pub mod pe_util;

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectMethod {
    LoadLibrary,
    Reflective,
    ManualMap,
}

impl std::fmt::Display for InjectMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InjectMethod::LoadLibrary => write!(f, "LoadLibraryW"),
            InjectMethod::Reflective => write!(f, "Reflective"),
            InjectMethod::ManualMap => write!(f, "ManualMap"),
        }
    }
}

#[derive(Debug)]
pub struct InjectResult {
    pub method: InjectMethod,
    pub base_address: usize,
    pub dll_path: String,
}

pub fn inject(pid: u32, dll_path: &Path, method: InjectMethod) -> Result<InjectResult, String> {
    match method {
        InjectMethod::LoadLibrary => loadlibrary::inject(pid, dll_path),
        InjectMethod::Reflective => reflective::inject(pid, dll_path),
        InjectMethod::ManualMap => manualmap::inject(pid, dll_path),
    }
}

pub fn is_process_64bit(pid: u32) -> Result<bool, String> {
    use windows::Win32::Foundation::{CloseHandle, BOOL};
    use windows::Win32::System::Threading::{IsWow64Process, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let handle = unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
    }.map_err(|e| format!("OpenProcess failed: {:?}", e))?;

    let mut is_wow64 = BOOL(0);
    let result = unsafe { IsWow64Process(handle, &mut is_wow64) };
    let _ = unsafe { CloseHandle(handle) };

    result.map_err(|e| format!("IsWow64Process failed: {:?}", e))?;
    Ok(is_wow64.0 == 0)
}