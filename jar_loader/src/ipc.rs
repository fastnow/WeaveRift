use std::mem;
use std::ptr;
use windows::Win32::Foundation::*;
use windows::Win32::System::Memory::{
    OpenFileMappingW, CreateFileMappingW, MapViewOfFile,
    FILE_MAP_READ, FILE_MAP_ALL_ACCESS, PAGE_READWRITE,
};
use windows::core::PCWSTR;

// ─── 玩家数据结构（Agent 写，DLL 读）────────

#[repr(C)]
pub struct PlayerData {
    pub magic: u32,       // 0x504C4159 "PLAY"
    pub valid: u32,       // 0 = 无效，1 = 有效
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub health: f32,
    pub max_health: f32,
    pub tps: f32,         // 服务端 TPS（可选）
}

const PLAYER_MAGIC: u32 = 0x504C4159;
const PLAYER_SHARED_NAME: &str = "Local\\WeaveRift.PlayerData";

// ─── 命令结构（DLL 写，Agent 读）────────────

#[repr(C)]
pub struct CommandData {
    pub magic: u32,        // 0x434D4421 "CMD!"
    pub seq: u32,          // 序号，每次写递增
    pub cmd_id: u32,       // 命令 ID
    pub arg: i32,          // 命令参数
    pub reserved: [u8; 16],
}

const COMMAND_MAGIC: u32 = 0x434D4421;
const COMMAND_SHARED_NAME: &str = "Local\\WeaveRift.Commands";

// 命令 ID
pub const CMD_KILLAURA_TOGGLE: u32 = 1;
pub const CMD_VELOCITY_TOGGLE: u32 = 2;
pub const CMD_ESP_TOGGLE: u32 = 3;
pub const CMD_SPEED_TOGGLE: u32 = 4;

// ─── 全局指针 ────────────────────────────────

static mut PLAYER_VIEW: *const PlayerData = ptr::null();
static mut COMMAND_VIEW: *mut CommandData = ptr::null_mut();

pub unsafe fn open_player_data() -> bool {
    let name_wide: Vec<u16> = PLAYER_SHARED_NAME.encode_utf16().chain(Some(0)).collect();
    let handle = OpenFileMappingW(FILE_MAP_READ.0, BOOL(0), PCWSTR(name_wide.as_ptr()));
    let h = match handle { Ok(h) => h, Err(_) => return false };

    let view = MapViewOfFile(h, FILE_MAP_READ, 0, 0, mem::size_of::<PlayerData>());
    if view.Value.is_null() { return false; }

    let data = view.Value as *const PlayerData;
    if (*data).magic != PLAYER_MAGIC { return false; }

    PLAYER_VIEW = data;
    true
}

pub unsafe fn open_command_data() -> bool {
    let name_wide: Vec<u16> = COMMAND_SHARED_NAME.encode_utf16().chain(Some(0)).collect();

    // 先尝试打开已存在的
    let handle = OpenFileMappingW(FILE_MAP_ALL_ACCESS.0, BOOL(0), PCWSTR(name_wide.as_ptr()));
    let h = match handle {
        Ok(h) => h,
        Err(_) => {
            // 不存在，创建
            match CreateFileMappingW(
                HANDLE::default(),
                None,
                PAGE_READWRITE,
                0,
                mem::size_of::<CommandData>() as u32,
                PCWSTR(name_wide.as_ptr()),
            ) {
                Ok(h) => h,
                Err(_) => return false,
            }
        }
    };

    let view = MapViewOfFile(h, FILE_MAP_ALL_ACCESS, 0, 0, mem::size_of::<CommandData>());
    if view.Value.is_null() { return false; }

    let data = view.Value as *mut CommandData;
    (*data).magic = COMMAND_MAGIC;
    (*data).seq = 0;

    COMMAND_VIEW = data;
    true
}

pub unsafe fn get_player() -> Option<(f64, f64, f64, f32, f32, f32, f32, f32)> {
    if PLAYER_VIEW.is_null() { return None; }
    let d = &*PLAYER_VIEW;
    if d.valid == 0 { return None; }
    Some((d.x, d.y, d.z, d.yaw, d.pitch, d.health, d.max_health, d.tps))
}

pub unsafe fn send_command(cmd_id: u32, arg: i32) {
    if COMMAND_VIEW.is_null() { return; }
    let d = &mut *COMMAND_VIEW;
    d.seq = d.seq.wrapping_add(1);
    d.cmd_id = cmd_id;
    d.arg = arg;
}