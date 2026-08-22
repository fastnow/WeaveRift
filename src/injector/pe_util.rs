use anyhow::{anyhow, Result};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::os::windows::ffi::OsStrExt;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::core::PCWSTR;

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

    fn rva_to_offset(&self, rva: u32) -> Option<usize> {
        if rva < self.nt.OptionalHeader.SizeOfHeaders {
            return Some(rva as usize);
        }
        for sec in &self.sections {
            let start = sec.VirtualAddress;
            let end = start + sec.SizeOfRawData;
            if rva >= start && rva < end {
                return Some((sec.PointerToRawData + (rva - start)) as usize);
            }
        }
        None
    }

    pub fn get_export_rva(&self, target: &str) -> Option<usize> {
        let export_dir = self.nt.OptionalHeader.DataDirectory[0];
        if export_dir.VirtualAddress == 0 || export_dir.Size == 0 {
            return None;
        }
        let offset = self.rva_to_offset(export_dir.VirtualAddress)?;
        let data = &self.data;
        if offset + 40 > data.len() { return None; }
        let base = offset;
        let num_names = u32::from_le_bytes(data[base+24..base+28].try_into().unwrap()) as usize;
        let addr_names = u32::from_le_bytes(data[base+32..base+36].try_into().unwrap());
        let addr_ordinals = u32::from_le_bytes(data[base+36..base+40].try_into().unwrap());
        let addr_funcs = u32::from_le_bytes(data[base+28..base+32].try_into().unwrap());
        let name_off = self.rva_to_offset(addr_names)?;
        let ord_off = self.rva_to_offset(addr_ordinals)?;
        let func_off = self.rva_to_offset(addr_funcs)?;

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

    pub fn fill_imports_remote(&self, handle: HANDLE, remote_base: *mut std::ffi::c_void) -> Result<(), String> {
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

            let wide: Vec<u16> = std::ffi::OsStr::new(dll_name).encode_wide().chain(Some(0)).collect();
            let hmod = unsafe { GetModuleHandleW(PCWSTR(wide.as_ptr())) };
            let hmod = match hmod {
                Ok(h) => h,
                Err(_) => unsafe {
                    LoadLibraryW(PCWSTR(wide.as_ptr()))
                        .map_err(|e| format!("Cannot load {}: {:?}", dll_name, e))?
                }
            };

            let thunk_rva = if desc.OriginalFirstThunk != 0 { desc.OriginalFirstThunk } else { desc.FirstThunk };
            let toff = self.rva_to_offset(thunk_rva).ok_or("Invalid thunk RVA")?;
            let iat_rva = desc.FirstThunk;
            let mut i = 0;

            loop {
                let tptr = toff + i * 8;
                if tptr + 8 > self.data.len() { break; }
                let tv = u64::from_le_bytes(self.data[tptr..tptr+8].try_into().unwrap());
                if tv == 0 { break; }

                let func_addr = if (tv & 0x8000000000000000) != 0 {
                    let ord = (tv & 0xFFFF) as u16;
                    unsafe { GetProcAddress(hmod, windows::core::PCSTR(ord as usize as *const u8)) }
                        .map(|a| a as u64).unwrap_or(0)
                } else {
                    let nrva = tv as u32;
                    let nptr = self.rva_to_offset(nrva).ok_or("Invalid import name RVA")?;
                    if nptr + 2 > self.data.len() { break; }
                    let mut nend = nptr + 2;
                    while nend < self.data.len() && self.data[nend] != 0 { nend += 1; }
                    let fname = std::str::from_utf8(&self.data[nptr+2..nend])
                        .map_err(|_| "Invalid function name")?;
                    unsafe { GetProcAddress(hmod, windows::core::PCSTR(fname.as_ptr())) }
                        .map(|a| a as u64).unwrap_or(0)
                };

                if func_addr == 0 {
                    return Err(format!("Cannot resolve {} import", dll_name));
                }

                let iat_addr = (remote_base as usize + iat_rva as usize + i * 8) as *mut std::ffi::c_void;
                let mut written = 0usize;
                unsafe {
                    WriteProcessMemory(handle, iat_addr, &func_addr as *const u64 as *const std::ffi::c_void, 8, Some(&mut written))
                        .map_err(|e| format!("IAT write failed: {:?}", e))?;
                }
                i += 1;
            }
            offset += std::mem::size_of::<IMAGE_IMPORT_DESCRIPTOR>();
        }
        Ok(())
    }
}