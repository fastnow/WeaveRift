use std::mem;
use std::ptr;
use windows::Win32::Foundation::*;
use windows::Win32::System::Memory::*;
use windows::core::PCWSTR;

#[repr(C)]
pub struct SharedPath {
    pub magic: u32,
    pub mode: u32,
    pub target_pid: u32,
    pub path: [u16; 260],
}

const SHARED_NAME: &str = "Local\\WeaveRift.Path";
const MAGIC: u32 = 0x57415645;

pub fn write_jar_path(path: &std::path::Path, target_pid: u32) -> Result<(), String> {
    let name_wide: Vec<u16> = SHARED_NAME.encode_utf16().chain(Some(0)).collect();
    let pcwstr = PCWSTR(name_wide.as_ptr());

    let handle = unsafe {
        CreateFileMappingW(
            HANDLE::default(),
            None,
            PAGE_READWRITE,
            0,
            mem::size_of::<SharedPath>() as u32,
            pcwstr,
        )
    };
    let h = handle.map_err(|_| "CreateFileMappingW failed".to_string())?;

    let view = unsafe {
        MapViewOfFile(h, FILE_MAP_ALL_ACCESS, 0, 0, mem::size_of::<SharedPath>())
    };
    if view.Value.is_null() {
        unsafe { let _ = CloseHandle(h); }
        return Err("MapViewOfFile failed".into());
    }

    let shared = view.Value as *mut SharedPath;
    unsafe {
        (*shared).magic = MAGIC;
        (*shared).mode = 0;
        (*shared).target_pid = target_pid;

        let mut wide_path: Vec<u16> = path.to_str()
            .ok_or("Invalid path")?
            .encode_utf16()
            .take(259)
            .collect();
        wide_path.push(0);

        let len = wide_path.len().min(260);
        let path_ptr = (*shared).path.as_mut_ptr();
        ptr::copy_nonoverlapping(wide_path.as_ptr(), path_ptr, len);
        if len < 260 {
            ptr::write_bytes(path_ptr.add(len), 0, 260 - len);
        }

        // 不关闭句柄，让进程退出时系统自动回收
    }
    Ok(())
}