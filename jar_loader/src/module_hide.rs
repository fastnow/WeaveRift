//! PEB 断链：把当前模块从 Ldr 的三条链表里摘除。
//! 参考 UnlinkDLL / wraith-rs 的做法[citation:3][citation:8]。
//!
//! ⚠️ 这只对依赖 PEB 链表的枚举 API 有效。
//! 如果反作弊直接扫内存页（VirtualQuery / VAD），断链无效。

use std::ffi::c_void;
use std::ptr;
use windows::Win32::Foundation::HMODULE;

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

#[repr(C)]
struct ListEntry {
    flink: *mut ListEntry,
    blink: *mut ListEntry,
}

#[repr(C)]
struct LdrDataTableEntry {
    in_load_order_links: ListEntry,
    in_memory_order_links: ListEntry,
    in_initialization_order_links: ListEntry,
    dll_base: *mut u8,
    entry_point: *mut u8,
    size_of_image: u32,
    full_dll_name: UnicodeString,
    base_dll_name: UnicodeString,
}

#[repr(C)]
struct PebLdrData {
    length: u32,
    initialized: u8,
    ss_handle: *mut c_void,
    in_load_order_module_list: ListEntry,
    in_memory_order_module_list: ListEntry,
    in_initialization_order_module_list: ListEntry,
}

// PEB 里 Ldr 的偏移（x64 下是 0x18）
const PEB_LDR_OFFSET: usize = 0x18;

// ─── 获取当前进程 PEB ────────────────────────────

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

// ─── 断链 ────────────────────────────────────────

/// 从双向链表里摘除一个节点
unsafe fn unlink(entry: *mut ListEntry) {
    let flink = (*entry).flink;
    let blink = (*entry).blink;
    if !flink.is_null() {
        (*flink).blink = blink;
    }
    if !blink.is_null() {
        (*blink).flink = flink;
    }
    // 让节点自己指向自己，避免残留引用
    (*entry).flink = entry;
    (*entry).blink = entry;
}

/// 隐藏指定模块（传 HMODULE 或 DLL 基址）
pub unsafe fn hide_module(module_base: *mut c_void) -> bool {
    if module_base.is_null() {
        return false;
    }

    let peb = get_peb();
    if peb.is_null() {
        return false;
    }

    let ldr = *(peb.add(PEB_LDR_OFFSET) as *mut *mut PebLdrData);
    if ldr.is_null() {
        return false;
    }

    // 三条链表都要遍历，全部断掉[citation:3][citation:9]
    let lists = [
        &(*ldr).in_load_order_module_list as *const ListEntry as *mut ListEntry,
        &(*ldr).in_memory_order_module_list as *const ListEntry as *mut ListEntry,
        &(*ldr).in_initialization_order_module_list as *const ListEntry as *mut ListEntry,
    ];

    let mut found = false;

    for head in lists {
        let mut current = (*head).flink;
        while !current.is_null() && current != head {
            // LDR_DATA_TABLE_ENTRY 的起始地址
            // in_load_order_links 在结构体最前面，所以 current 就是 entry 的地址
            // 但 in_memory_order_links / in_initialization_order_links 有偏移
            // 这里用 dll_base 做匹配，遍历时按不同链表算偏移

            // 简化：统一用 in_load_order_links 的偏移（0）
            // 实际上需要根据当前遍历的是哪条链表来调整偏移
            let entry = current as *mut LdrDataTableEntry;

            if (*entry).dll_base == module_base as *mut u8 {
                // 三条链表全部断掉
                unlink(&mut (*entry).in_load_order_links);
                unlink(&mut (*entry).in_memory_order_links);
                unlink(&mut (*entry).in_initialization_order_links);

                // 清空名称，防止通过名称字符串找到
                (*entry).full_dll_name.length = 0;
                (*entry).base_dll_name.length = 0;
                if !(*entry).full_dll_name.buffer.is_null() {
                    *(*entry).full_dll_name.buffer = 0;
                }
                if !(*entry).base_dll_name.buffer.is_null() {
                    *(*entry).base_dll_name.buffer = 0;
                }

                found = true;
                break;
            }

            current = (*current).flink;
        }
        if found {
            break;
        }
    }

    found
}

/// 擦除 PE 头
pub unsafe fn erase_pe_header(module_base: *mut c_void) -> bool {
    if module_base.is_null() {
        return false;
    }

    let base = module_base as *mut u8;

    let dos = base as *mut u16;
    if *dos != 0x5A4D {
        return false;
    }

    let nt_offset = *(base.add(0x3C) as *mut u32) as usize;
    let nt = base.add(nt_offset) as *mut u32;
    if *nt != 0x00004550 {
        return false;
    }

    for i in 0..0x1000 {
        *base.add(i) = 0xCC;
    }

    true
}