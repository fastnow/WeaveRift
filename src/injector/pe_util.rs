use anyhow::{anyhow, Result};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::os::windows::ffi::OsStrExt;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::core::{PCSTR, PCWSTR};
use std::path::Path;
use std::fs;
use crate::error::InjectorError;

// 引入 winapi 的 tlhelp32 和 handleapi
use winapi::um::tlhelp32::{CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W, TH32CS_SNAPMODULE};
use winapi::um::libloaderapi::FreeLibrary;
use winapi::um::handleapi::{CloseHandle as WinapiCloseHandle, INVALID_HANDLE_VALUE};
use winapi::shared::minwindef::HMODULE as WinapiHMODULE;

// ─── PE 结构体定义 ─────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct IMAGE_DOS_HEADER {
    e_magic: u16, e_cblp: u16, e_cp: u16, e_crlc: u16, e_cparhdr: u16,
    e_minalloc: u16, e_maxalloc: u16, e_ss: u16, e_sp: u16, e_csum: u16,
    e_ip: u16, e_cs: u16, e_lfarlc: u16, e_ovno: u16, e_res: [u16; 4],
    e_oemid: u16, e_oeminfo: u16, e_res2: [u16; 10], e_lfanew: i32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct IMAGE_FILE_HEADER {
    Machine: u16, NumberOfSections: u16, TimeDateStamp: u32,
    PointerToSymbolTable: u32, NumberOfSymbols: u32,
    SizeOfOptionalHeader: u16, Characteristics: u16,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct IMAGE_DATA_DIRECTORY {
    VirtualAddress: u32, Size: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct IMAGE_OPTIONAL_HEADER64 {
    Magic: u16, MajorLinkerVersion: u8, MinorLinkerVersion: u8,
    SizeOfCode: u32, SizeOfInitializedData: u32, SizeOfUninitializedData: u32,
    AddressOfEntryPoint: u32, BaseOfCode: u32,
    ImageBase: u64, SectionAlignment: u32, FileAlignment: u32,
    MajorOperatingSystemVersion: u16, MinorOperatingSystemVersion: u16,
    MajorImageVersion: u16, MinorImageVersion: u16,
    MajorSubsystemVersion: u16, MinorSubsystemVersion: u16,
    Win32VersionValue: u32, SizeOfImage: u32, SizeOfHeaders: u32,
    CheckSum: u32, Subsystem: u16, DllCharacteristics: u16,
    SizeOfStackReserve: u64, SizeOfStackCommit: u64,
    SizeOfHeapReserve: u64, SizeOfHeapCommit: u64,
    LoaderFlags: u32, NumberOfRvaAndSizes: u32,
    DataDirectory: [IMAGE_DATA_DIRECTORY; 16],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct IMAGE_NT_HEADERS64 {
    Signature: u32,
    FileHeader: IMAGE_FILE_HEADER,
    OptionalHeader: IMAGE_OPTIONAL_HEADER64,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct IMAGE_SECTION_HEADER {
    pub Name: [u8; 8],
    pub VirtualSize: u32,
    pub VirtualAddress: u32,
    pub SizeOfRawData: u32,
    pub PointerToRawData: u32,
    pub PointerToRelocations: u32,
    pub PointerToLinenumbers: u32,
    pub NumberOfRelocations: u16,
    pub NumberOfLinenumbers: u16,
    pub Characteristics: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct IMAGE_BASE_RELOCATION {
    VirtualAddress: u32,
    SizeOfBlock: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct IMAGE_IMPORT_DESCRIPTOR {
    OriginalFirstThunk: u32,
    TimeDateStamp: u32,
    ForwarderChain: u32,
    Name: u32,
    FirstThunk: u32,
}

const IMAGE_DOS_SIGNATURE: u16 = 0x5A4D;
const IMAGE_NT_SIGNATURE: u32 = 0x00004550;
const IMAGE_REL_BASED_DIR64: u16 = 10;
const IMAGE_REL_BASED_HIGHLOW: u16 = 3;
const IMAGE_ORDINAL_FLAG64: u64 = 0x8000000000000000;

/// 等价于 C 宏 MAKEINTRESOURCEA：把导入序号(ordinal)直接当作"资源字符串指针"传给
/// GetProcAddress。GetProcAddress 会依据"该指针高 16 位为 0"判定为按序号导入，
/// 从而避免把小序号当普通内存地址去解引用。
fn make_int_resource_a(ord: u16) -> PCSTR {
    PCSTR(ord as usize as *const u8)
}

// ─── PE 解析器 ─────────────────────────────────────────────────────────────────

pub struct PeParser {
    pub data: Vec<u8>,
    dos: IMAGE_DOS_HEADER,
    pub nt: IMAGE_NT_HEADERS64,
    pub sections: Vec<IMAGE_SECTION_HEADER>,
}

impl PeParser {
    pub fn new(data: Vec<u8>) -> Result<Self> {
        let mut cursor = Cursor::new(&data);
        let mut dos: IMAGE_DOS_HEADER = unsafe { std::mem::zeroed() };
        cursor.read_exact(unsafe {
            std::slice::from_raw_parts_mut(&mut dos as *mut _ as *mut u8, std::mem::size_of::<IMAGE_DOS_HEADER>())
        })?;
        if dos.e_magic != IMAGE_DOS_SIGNATURE {
            return Err(anyhow!("Invalid DOS signature"));
        }
        cursor.seek(SeekFrom::Start(dos.e_lfanew as u64))?;
        let mut nt: IMAGE_NT_HEADERS64 = unsafe { std::mem::zeroed() };
        cursor.read_exact(unsafe {
            std::slice::from_raw_parts_mut(&mut nt as *mut _ as *mut u8, std::mem::size_of::<IMAGE_NT_HEADERS64>())
        })?;
        if nt.Signature != IMAGE_NT_SIGNATURE {
            return Err(anyhow!("Invalid NT signature"));
        }
        if nt.OptionalHeader.Magic == 0x10b {
            return Err(anyhow!("32-bit PE detected — not supported in this version"));
        }
        let num = nt.FileHeader.NumberOfSections as usize;
        let mut sections = Vec::with_capacity(num);
        for _ in 0..num {
            let mut sec: IMAGE_SECTION_HEADER = unsafe { std::mem::zeroed() };
            cursor.read_exact(unsafe {
                std::slice::from_raw_parts_mut(&mut sec as *mut _ as *mut u8, std::mem::size_of::<IMAGE_SECTION_HEADER>())
            })?;
            sections.push(sec);
        }
        Ok(PeParser { data, dos, nt, sections })
    }

    pub fn is_64bit(&self) -> bool {
        self.nt.OptionalHeader.Magic == 0x20B
    }

    pub fn size_of_image(&self) -> usize {
        self.nt.OptionalHeader.SizeOfImage as usize
    }

    pub fn size_of_headers(&self) -> usize {
        self.nt.OptionalHeader.SizeOfHeaders as usize
    }

    pub fn image_base(&self) -> usize {
        self.nt.OptionalHeader.ImageBase as usize
    }

    pub fn entry_point_rva(&self) -> usize {
        self.nt.OptionalHeader.AddressOfEntryPoint as usize
    }

    pub fn sections(&self) -> &[IMAGE_SECTION_HEADER] {
        &self.sections
    }

    // 使用 VirtualSize 判断边界
    pub fn rva_to_offset(&self, rva: u32) -> Option<usize> {
        if rva < self.nt.OptionalHeader.SizeOfHeaders {
            return Some(rva as usize);
        }
        for sec in &self.sections {
            let start = sec.VirtualAddress;
            let end = start + sec.VirtualSize;
            if rva >= start && rva < end {
                if sec.SizeOfRawData == 0 { return None; }
                return Some((sec.PointerToRawData + (rva - start)) as usize);
            }
        }
        None
    }

    // 支持名称和序号导出
    pub fn get_export_rva(&self, target: &str) -> Option<usize> {
        let export_dir = self.nt.OptionalHeader.DataDirectory[0];
        if export_dir.VirtualAddress == 0 || export_dir.Size == 0 {
            return None;
        }
        let offset = self.rva_to_offset(export_dir.VirtualAddress)?;
        let data = &self.data;
        if offset + 40 > data.len() { return None; }
        let base = offset;
        let _num_names = u32::from_le_bytes(data[base+24..base+28].try_into().unwrap()) as usize;
        let num_funcs = u32::from_le_bytes(data[base+20..base+24].try_into().unwrap()) as usize;
        let addr_names = u32::from_le_bytes(data[base+32..base+36].try_into().unwrap());
        let addr_ordinals = u32::from_le_bytes(data[base+36..base+40].try_into().unwrap());
        let addr_funcs = u32::from_le_bytes(data[base+28..base+32].try_into().unwrap());

        if let Some(rva) = self.find_export_by_name(addr_names, addr_ordinals, addr_funcs, target) {
            return Some(rva);
        }
        if let Ok(ord) = target.parse::<u16>() {
            return self.find_export_by_ordinal(addr_funcs, num_funcs, ord);
        }
        None
    }

    fn find_export_by_name(&self, addr_names: u32, addr_ordinals: u32, addr_funcs: u32, target: &str) -> Option<usize> {
        let name_off = self.rva_to_offset(addr_names)?;
        let ord_off = self.rva_to_offset(addr_ordinals)?;
        let func_off = self.rva_to_offset(addr_funcs)?;
        let data = &self.data;
        let num_names = (self.nt.OptionalHeader.DataDirectory[0].Size / 4) as usize;
        for i in 0..num_names {
            let nrva_ptr = name_off + i * 4;
            if nrva_ptr + 4 > data.len() { continue; }
            let nrva = u32::from_le_bytes(data[nrva_ptr..nrva_ptr+4].try_into().unwrap());
            let nptr = self.rva_to_offset(nrva)?;
            let mut end = nptr;
            while end < data.len() && data[end] != 0 { end += 1; }
            let name = std::str::from_utf8(&data[nptr..end]).unwrap_or("");
            if name == target {
                let optr = ord_off + i * 2;
                if optr + 2 > data.len() { continue; }
                let ord = u16::from_le_bytes(data[optr..optr+2].try_into().unwrap()) as usize;
                let fptr = func_off + ord * 4;
                if fptr + 4 > data.len() { continue; }
                let frva = u32::from_le_bytes(data[fptr..fptr+4].try_into().unwrap());
                return Some(frva as usize);
            }
        }
        None
    }

    fn find_export_by_ordinal(&self, addr_funcs: u32, num_funcs: usize, ord: u16) -> Option<usize> {
        let func_off = self.rva_to_offset(addr_funcs)?;
        let data = &self.data;
        let idx = ord as usize;
        if idx >= num_funcs { return None; }
        let fptr = func_off + idx * 4;
        if fptr + 4 > data.len() { return None; }
        let frva = u32::from_le_bytes(data[fptr..fptr+4].try_into().unwrap());
        Some(frva as usize)
    }

    // 重定位处理
    pub fn apply_relocations_remote(&self, handle: HANDLE, remote_base: *mut std::ffi::c_void, delta: isize) -> Result<(), String> {
        let reloc_dir = self.nt.OptionalHeader.DataDirectory[5];
        if reloc_dir.VirtualAddress == 0 || reloc_dir.Size == 0 { return Ok(()); }
        let mut offset = self.rva_to_offset(reloc_dir.VirtualAddress)
            .ok_or("Invalid relocation RVA")?;
        let end = offset + reloc_dir.Size as usize;

        while offset < end {
            if offset + 8 > self.data.len() { break; }
            let block: IMAGE_BASE_RELOCATION = unsafe {
                std::ptr::read_unaligned(self.data.as_ptr().add(offset) as *const IMAGE_BASE_RELOCATION)
            };
            if block.SizeOfBlock == 0 { break; }
            let entries = (block.SizeOfBlock as usize - 8) / 2;
            let eoff = offset + 8;
            for i in 0..entries {
                let ep = eoff + i * 2;
                if ep + 2 > self.data.len() { continue; }
                let entry = u16::from_le_bytes(self.data[ep..ep+2].try_into().unwrap());
                let ty = (entry >> 12) & 0xF;
                let rva = (entry & 0xFFF) as usize;
                let addr = block.VirtualAddress as usize + rva;
                let remote_addr = (remote_base as usize + addr) as *mut std::ffi::c_void;

                match ty {
                    IMAGE_REL_BASED_DIR64 => {
                        let mut buf = [0u8; 8];
                        let mut read = 0usize;
                        unsafe {
                            ReadProcessMemory(handle, remote_addr, buf.as_mut_ptr() as *mut std::ffi::c_void, 8, Some(&mut read))
                                .map_err(|e| format!("Reloc read failed: {:?}", e))?;
                        }
                        let old = u64::from_le_bytes(buf);
                        let new = (old as isize + delta) as u64;
                        let mut written = 0usize;
                        unsafe {
                            WriteProcessMemory(handle, remote_addr, new.to_le_bytes().as_ptr() as *const std::ffi::c_void, 8, Some(&mut written))
                                .map_err(|e| format!("Reloc write failed: {:?}", e))?;
                        }
                    }
                    IMAGE_REL_BASED_HIGHLOW => {
                        let mut buf = [0u8; 4];
                        let mut read = 0usize;
                        unsafe {
                            ReadProcessMemory(handle, remote_addr, buf.as_mut_ptr() as *mut std::ffi::c_void, 4, Some(&mut read))
                                .map_err(|e| format!("Reloc read failed: {:?}", e))?;
                        }
                        let old = u32::from_le_bytes(buf);
                        let new = (old as isize + delta) as u32;
                        let mut written = 0usize;
                        unsafe {
                            WriteProcessMemory(handle, remote_addr, new.to_le_bytes().as_ptr() as *const std::ffi::c_void, 4, Some(&mut written))
                                .map_err(|e| format!("Reloc write failed: {:?}", e))?;
                        }
                    }
                    _ => {}
                }
            }
            offset += block.SizeOfBlock as usize;
        }
        Ok(())
    }

    // 新版导入填充（在目标进程中解析地址）
    pub fn fill_imports_remote_fixed(&self, pid: u32, handle: HANDLE, remote_base: *mut std::ffi::c_void) -> Result<(), String> {
        let import_dir = self.nt.OptionalHeader.DataDirectory[1];
        if import_dir.VirtualAddress == 0 || import_dir.Size == 0 { return Ok(()); }
        let mut offset = self.rva_to_offset(import_dir.VirtualAddress)
            .ok_or("Invalid import RVA")?;
        let end = offset + import_dir.Size as usize;

        while offset < end {
            if offset + std::mem::size_of::<IMAGE_IMPORT_DESCRIPTOR>() > self.data.len() { break; }
            let desc: IMAGE_IMPORT_DESCRIPTOR = unsafe {
                std::ptr::read_unaligned(self.data.as_ptr().add(offset) as *const IMAGE_IMPORT_DESCRIPTOR)
            };
            if desc.OriginalFirstThunk == 0 && desc.FirstThunk == 0 { break; }

            let noff = self.rva_to_offset(desc.Name).ok_or("Invalid DLL name RVA")?;
            let mut nend = noff;
            while nend < self.data.len() && self.data[nend] != 0 { nend += 1; }
            let dll_name = std::str::from_utf8(&self.data[noff..nend])
                .map_err(|_| "Invalid DLL name")?;

            // 获取目标进程中该 DLL 的基址，若未加载则远程加载
            let module_base = match Self::get_remote_module_base(pid, dll_name) {
                Ok(base) => base,
                Err(_) => {
                    Self::remote_load_library(pid, dll_name)
                        .map_err(|e| format!("Failed to load {}: {}", dll_name, e))?
                }
            };

            // 在注入器进程中加载该 DLL 以解析导出表（临时）
            let wide: Vec<u16> = std::ffi::OsStr::new(dll_name).encode_wide().chain(Some(0)).collect();
            let local_mod = unsafe { LoadLibraryW(PCWSTR(wide.as_ptr())) }
                .map_err(|e| format!("LoadLibraryW local failed for {}: {:?}", dll_name, e))?;
            // 将 windows 的 HMODULE 转为 winapi 的 HMODULE
            let local_mod_winapi = local_mod.0 as WinapiHMODULE;
            let local_base = unsafe { GetModuleHandleW(PCWSTR(wide.as_ptr())) }
                .map_err(|_| "GetModuleHandleW local failed")? .0 as usize;

            let thunk_rva = if desc.OriginalFirstThunk != 0 { desc.OriginalFirstThunk } else { desc.FirstThunk };
            let toff = self.rva_to_offset(thunk_rva).ok_or("Invalid thunk RVA")?;
            let iat_rva = desc.FirstThunk;
            let mut i = 0;

            loop {
                let tptr = toff + i * 8;
                if tptr + 8 > self.data.len() { break; }
                let tv = u64::from_le_bytes(self.data[tptr..tptr+8].try_into().unwrap());
                if tv == 0 { break; }

                let func_rva = if (tv & IMAGE_ORDINAL_FLAG64) != 0 {
                    let ord = (tv & 0xFFFF) as u16;
                    let addr = unsafe { GetProcAddress(local_mod, make_int_resource_a(ord)) }
                        .ok_or("GetProcAddress by ordinal failed")? as usize;
                    addr - local_base
                } else {
                    let nrva = tv as u32;
                    let nptr = self.rva_to_offset(nrva).ok_or("Invalid import name RVA")?;
                    if nptr + 2 > self.data.len() { break; }
                    let mut nend = nptr + 2;
                    while nend < self.data.len() && self.data[nend] != 0 { nend += 1; }
                    let fname = std::str::from_utf8(&self.data[nptr+2..nend])
                        .map_err(|_| "Invalid function name")?;
                    let addr = unsafe { GetProcAddress(local_mod, PCSTR(fname.as_ptr())) }
                        .ok_or_else(|| format!("GetProcAddress {} not found", fname))? as usize;
                    addr - local_base
                };

                let target_addr = module_base + func_rva;
                let iat_addr = (remote_base as usize + iat_rva as usize + i * 8) as *mut std::ffi::c_void;
                let mut written = 0usize;
                unsafe {
                    WriteProcessMemory(handle, iat_addr, &target_addr as *const usize as *const std::ffi::c_void, 8, Some(&mut written))
                        .map_err(|e| format!("IAT write failed: {:?}", e))?;
                }
                i += 1;
            }
            // 释放本地加载的 DLL（使用 winapi 的 FreeLibrary）
            unsafe { FreeLibrary(local_mod_winapi); }
            offset += std::mem::size_of::<IMAGE_IMPORT_DESCRIPTOR>();
        }
        Ok(())
    }

    // 获取远程进程模块基址（使用 winapi 的 Toolhelp）
    pub fn get_remote_module_base(pid: u32, dll_name: &str) -> Result<usize, InjectorError> {
        use winapi::shared::minwindef::DWORD;

        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid as DWORD) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(InjectorError::CommunicationError("Failed to create snapshot".into()));
        }

        let mut entry: MODULEENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;

        let ok = unsafe { Module32FirstW(snapshot, &mut entry) };
        if ok == 0 {
            unsafe { WinapiCloseHandle(snapshot); }
            return Err(InjectorError::CommunicationError("Module32First failed".into()));
        }

        let mut found = None;
        loop {
            let name_wide = &entry.szModule;
            let mut len = 0;
            while len < 256 && name_wide[len] != 0 { len += 1; }
            let name = String::from_utf16_lossy(&name_wide[..len]);
            if name.eq_ignore_ascii_case(dll_name) {
                found = Some(entry.modBaseAddr as usize);
                break;
            }
            if unsafe { Module32NextW(snapshot, &mut entry) } == 0 {
                break;
            }
        }

        unsafe { WinapiCloseHandle(snapshot); }
        found.ok_or(InjectorError::CommunicationError(format!("Module {} not found in target", dll_name)))
    }

    // 远程加载 DLL（使用 windows 的 LoadLibraryW 远程注入）
    pub fn remote_load_library(pid: u32, dll_name: &str) -> Result<usize, InjectorError> {
        use windows::Win32::System::Memory::{VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE};
        use windows::Win32::System::Threading::{CreateRemoteThread, WaitForSingleObject, GetExitCodeThread, OpenProcess, PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_WRITE, PROCESS_VM_READ};
        use std::ffi::OsStr;

        let handle = unsafe {
            OpenProcess(PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION | PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_VM_READ, false, pid)
        }.map_err(|_| InjectorError::ProcessNotFound("Cannot open process".into()))?;

        let wide: Vec<u16> = OsStr::new(dll_name).encode_wide().chain(Some(0)).collect();
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
                .map_err(|_| InjectorError::CommunicationError("WriteProcessMemory failed".into()))?;
        }

        let k32 = "kernel32.dll\0".encode_utf16().collect::<Vec<_>>();
        let hmod = unsafe { GetModuleHandleW(PCWSTR(k32.as_ptr())) }
            .map_err(|_| InjectorError::CommunicationError("GetModuleHandleW failed".into()))?;
        let addr = unsafe { GetProcAddress(hmod, PCSTR("LoadLibraryW\0".as_ptr())) }
            .ok_or(InjectorError::ExportNotFound("LoadLibraryW".into()))? as *const std::ffi::c_void;

        let thread = unsafe {
            CreateRemoteThread(handle, None, 0, Some(std::mem::transmute(addr)), Some(remote), 0, None)
        };
        let th = thread.map_err(|_| InjectorError::CommunicationError("CreateRemoteThread failed".into()))?;

        let wait = unsafe { WaitForSingleObject(th, 30000) };
        if wait == windows::Win32::Foundation::WAIT_TIMEOUT {
            let _ = unsafe { CloseHandle(th); VirtualFreeEx(handle, remote, 0, MEM_RELEASE); CloseHandle(handle); };
            return Err(InjectorError::Timeout);
        }
        let mut code = 0u32;
        unsafe { GetExitCodeThread(th, &mut code).ok(); }
        let _ = unsafe { CloseHandle(th); VirtualFreeEx(handle, remote, 0, MEM_RELEASE); CloseHandle(handle); };
        if code == 0 {
            Err(InjectorError::DllLoadFailed(format!("LoadLibraryW returned NULL for {}", dll_name)))
        } else {
            Ok(code as usize)
        }
    }
}

// 公共函数：检查 DLL 位数是否匹配目标进程。
// 注意：当前 ManualMap / Reflective 实现仅支持 64 位，因此这里明确拒绝 32 位，
// 避免 UI 放行后底层再报 "Not 64-bit" 的自相矛盾。
pub fn check_dll_architecture(dll_path: &Path, target_pid: u32) -> Result<(), InjectorError> {
    let data = fs::read(dll_path).map_err(|e| InjectorError::PeParseFailed(e.to_string()))?;
    let pe = PeParser::new(data).map_err(|e| InjectorError::PeParseFailed(e.to_string()))?;
    if !pe.is_64bit() {
        return Err(InjectorError::ArchMismatch("Only 64-bit DLLs are supported in this build".into()));
    }
    let target_64 = super::is_process_64bit(target_pid)?;
    if !target_64 {
        return Err(InjectorError::ArchMismatch("Only 64-bit target processes are supported in this build".into()));
    }
    Ok(())
}