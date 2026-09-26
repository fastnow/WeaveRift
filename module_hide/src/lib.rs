use std::ptr;
use std::mem;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::core::{PCSTR, PCWSTR};

#[repr(C)]
struct PROCESS_BASIC_INFORMATION {
    ExitStatus: i32,
    PebBaseAddress: *mut u8,
    AffinityMask: usize,
    BasePriority: i32,
    UniqueProcessId: usize,
    InheritedFromUniqueProcessId: usize,
}

type NTSTATUS = i32;
const STATUS_SUCCESS: NTSTATUS = 0;

type NtQueryInformationProcessFn = unsafe extern "system" fn(
    ProcessHandle: HANDLE,
    ProcessInformationClass: u32,
    ProcessInformation: *mut std::ffi::c_void,
    ProcessInformationLength: u32,
    ReturnLength: *mut u32,
) -> NTSTATUS;

fn get_nt_query_info_proc() -> Option<NtQueryInformationProcessFn> {
    let ntdll: Vec<u16> = "ntdll.dll\0".encode_utf16().collect();
    let hmod = unsafe { GetModuleHandleW(PCWSTR(ntdll.as_ptr())) }.ok()?;
    let fptr = unsafe { GetProcAddress(hmod, PCSTR("NtQueryInformationProcess\0".as_ptr())) }?;
    Some(unsafe { mem::transmute(fptr) })
}

unsafe fn get_peb_address() -> *mut u8 {
    let Some(nt_query) = get_nt_query_info_proc() else {
        #[cfg(target_arch = "x86_64")]
        {
            let peb: *mut u8;
            std::arch::asm!("mov {}, gs:[0x60]", out(reg) peb, options(nostack, preserves_flags));
            return peb;
        }
        #[cfg(target_arch = "x86")]
        {
            let peb: *mut u8;
            std::arch::asm!("mov {}, fs:[0x30]", out(reg) peb, options(nostack, preserves_flags));
            return peb;
        }
    };

    let handle = GetCurrentProcess();
    let mut pbi: PROCESS_BASIC_INFORMATION = mem::zeroed();
    let mut return_len = 0u32;
    let status = nt_query(
        handle,
        0,
        &mut pbi as *mut _ as *mut std::ffi::c_void,
        mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
        &mut return_len,
    );
    if status != STATUS_SUCCESS {
        return ptr::null_mut();
    }
    pbi.PebBaseAddress
}

#[repr(C)]
struct ListEntry {
    flink: *mut ListEntry,
    blink: *mut ListEntry,
}

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

#[repr(C)]
struct LdrDataTableEntry {
    in_load_order_links: ListEntry,
    in_memory_order_links: ListEntry,
    in_initialization_order_links: ListEntry,
    dll_base: *mut u8,
    _entry_point: *mut u8,
    _size_of_image: u32,
    _reserved: u32,
    full_dll_name: UnicodeString,
    base_dll_name: UnicodeString,
}

unsafe fn unlink_list(entry: *mut ListEntry) {
    let flink = (*entry).flink;
    let blink = (*entry).blink;
    if !flink.is_null() { (*flink).blink = blink; }
    if !blink.is_null() { (*blink).flink = flink; }
    (*entry).flink = entry;
    (*entry).blink = entry;
}

unsafe fn clear_name(s: *mut UnicodeString) {
    let len = (*s).length as usize / 2;
    if !(*s).buffer.is_null() && len > 0 {
        std::ptr::write_bytes((*s).buffer, 0, len);
    }
    (*s).length = 0;
    (*s).maximum_length = 0;
}

pub unsafe fn hide_module(hinst: *mut std::ffi::c_void) -> Result<(), String> {
    if hinst.is_null() {
        return Err("hinst is null".into());
    }

    let peb = get_peb_address();
    if peb.is_null() {
        return Err("PEB address not found".into());
    }

    let ldr = *(peb.add(0x18) as *mut *mut u8);
    if ldr.is_null() {
        return Err("PEB_LDR_DATA not found".into());
    }

    // 三条链表头
    let heads: [*mut ListEntry; 3] = [
        ldr.add(0x10) as *mut ListEntry,
        ldr.add(0x20) as *mut ListEntry,
        ldr.add(0x30) as *mut ListEntry,
    ];

    for head in heads {
        let mut current = (*head).flink;
        while !current.is_null() && current != head {
            let entry = (current as usize - 0x10) as *mut LdrDataTableEntry;
            if (*entry).dll_base == hinst as *mut u8 {
                unlink_list(&mut (*entry).in_load_order_links);
                unlink_list(&mut (*entry).in_memory_order_links);
                unlink_list(&mut (*entry).in_initialization_order_links);
                clear_name(&mut (*entry).full_dll_name);
                clear_name(&mut (*entry).base_dll_name);
                return Ok(());
            }
            current = (*current).flink;
        }
    }
    Err("Module not found in PEB lists".into())
}