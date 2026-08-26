//! 反作弊测试用：把被注入进目标进程的 DLL 从 PEB 加载器链表里摘除，
//! 使 `GetModuleHandle`、模块枚举（Process Hacker / NtQuerySystemInformation）等
//! 无法再发现这份模块 —— 这是市面上注入样本最常见的收尾手段之一。
//!
//! 注意：这只影响"用户态"的模块可见性，属于公开、大量文献讨论的经典技术。

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

// LDR_DATA_TABLE_ENTRY（x64 关键偏移）
#[repr(C)]
struct LdrDataTableEntry {
    in_load_order_links: ListEntry,            // +0x00
    in_memory_order_links: ListEntry,          // +0x10
    in_initialization_order_links: ListEntry,  // +0x20
    dll_base: *mut u8,                         // +0x30
    _entry_point: *mut u8,                     // +0x38
    _size_of_image: u32,                       // +0x40
    _reserved: u32,
    full_dll_name: UnicodeString,              // +0x58
    base_dll_name: UnicodeString,              // +0x68
}

#[cfg(target_arch = "x86_64")]
unsafe fn get_peb() -> *mut u8 {
    let peb: *mut u8;
    // x64 下 PEB 位于 GS:[0x60]
    std::arch::asm!("mov {}, gs:[0x60]", out(reg) peb, options(nostack, preserves_flags));
    peb
}

/// 把单个 LIST_ENTRY 从链表中断开（自身 flink/blink 回环指向自己，避免悬空）。
unsafe fn unlink_list(entry: *mut ListEntry) {
    let flink = (*entry).flink;
    let blink = (*entry).blink;
    if !flink.is_null() { (*flink).blink = blink; }
    if !blink.is_null() { (*blink).flink = flink; }
    (*entry).flink = entry;
    (*entry).blink = entry;
}

unsafe fn clear_name(s: *mut UnicodeString) {
    (*s).length = 0;
    (*s).maximum_length = 0;
    if !(*s).buffer.is_null() { *(*s).buffer = 0; }
}

/// 把指定模块（通常是被注入 DLL 自身的基址/hinst）从目标进程 PEB 的
/// InLoadOrder / InMemoryOrder / InInitializationOrder 三条加载链表中摘除，
/// 并清空其名字字符串。
///
/// # Safety
/// 必须在**被注入 DLL 自己的进程上下文**里调用（读取的是调用线程所在进程的 PEB），
/// 且 `hinst` 必须是该进程内真实存在的一个模块基址。
pub unsafe fn hide_module(hinst: *mut std::ffi::c_void) {
    if hinst.is_null() { return; }

    let peb = get_peb();
    if peb.is_null() { return; }

    let ldr = *(peb.add(0x18) as *mut *mut u8); // PEB->Ldr
    if ldr.is_null() { return; }

    // PEB_LDR_DATA.InMemoryOrderModuleList 位于 +0x20
    let head = ldr.add(0x20) as *mut ListEntry;
    let mut current = (*head).flink;

    while !current.is_null() && current != head {
        // InMemoryOrder 链上的元素指向 LDR_DATA_TABLE_ENTRY+0x10
        let entry = (current as usize - 0x10) as *mut LdrDataTableEntry;
        if (*entry).dll_base == hinst as *mut u8 {
            unlink_list(&mut (*entry).in_load_order_links as *mut ListEntry);
            unlink_list(&mut (*entry).in_memory_order_links as *mut ListEntry);
            unlink_list(&mut (*entry).in_initialization_order_links as *mut ListEntry);
            clear_name(&mut (*entry).full_dll_name);
            clear_name(&mut (*entry).base_dll_name);
            return;
        }
        current = (*current).flink;
    }
}
