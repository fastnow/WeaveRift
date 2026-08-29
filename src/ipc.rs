use std::mem;
use std::ptr;
use windows::Win32::Foundation::*;
use windows::Win32::System::Memory::*;
use windows::core::PCWSTR;

#[repr(C)]
struct SharedPath {
    magic: u32,
    mode: u32,
    path: [u16; 260],
}

const SHARED_NAME: &str = "Global\\WeaveRift.Path";
const MAGIC: u32 = 0x57415645;

pub fn write_agent_path(path: &std::path::Path) -> Result<(), String> {
    let name_wide: Vec<u16> = SHARED_NAME.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe {
        CreateFileMappingW(
            HANDLE::default(),
            None,
            PAGE_READWRITE,
            0,
            mem::size_of::<SharedPath>() as u32,
            PCWSTR(name_wide.as_ptr()),
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
        (*shared).mode = 1;

        let mut wide_path: Vec<u16> = path.to_str()
            .ok_or("Invalid path")?
            .encode_utf16()
            .take(259)
            .collect();
        wide_path.push(0);

        let len = wide_path.len().min(260);
        // 直接操作原始指针，不产生引用
        let path_ptr = (*shared).path.as_mut_ptr();
        ptr::copy_nonoverlapping(wide_path.as_ptr(), path_ptr, len);
        if len < 260 {
            ptr::write_bytes(path_ptr.add(len), 0, 260 - len);
        }

        let _ = UnmapViewOfFile(view);
        let _ = CloseHandle(h);
    }
    Ok(())
}