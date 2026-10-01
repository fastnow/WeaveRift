//! JNI 桥：从渲染线程直接读数据。

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use jni::objects::{JClass, JStaticMethodID, JValue};
use jni::signature::{Primitive, ReturnType};
use jni::sys::{
    jdouble, jint, jsize, JNIEnv as RawJNIEnv,
};
use jni::{JNIEnv, JavaVM};

use crate::logger;

static VM: OnceLock<JavaVM> = OnceLock::new();
static VM_PTR: AtomicIsize = AtomicIsize::new(0);

// ★ 用 AtomicIsize 存裸指针，绕过 Send/Sync
static BRIDGE_CLASS: AtomicIsize = AtomicIsize::new(0);
static SNAPSHOT_MID: AtomicIsize = AtomicIsize::new(0);
static COMMAND_MID: AtomicIsize = AtomicIsize::new(0);

// ─────────────────── JavaVM 定位 ───────────────────

pub unsafe fn find_vm() -> Result<(), String> {
    if VM.get().is_some() {
        return Ok(());
    }

    let hmod = windows::Win32::System::LibraryLoader::GetModuleHandleA(
        windows::core::PCSTR(b"jvm.dll\0".as_ptr()),
    )
    .map_err(|_| "jvm.dll 未加载（找错进程？）")?;

    let p = windows::Win32::System::LibraryLoader::GetProcAddress(
        hmod,
        windows::core::PCSTR(b"JNI_GetCreatedJavaVMs\0".as_ptr()),
    )
    .ok_or("JNI_GetCreatedJavaVMs 未导出")?;

    type GetVMs = unsafe extern "system" fn(*mut *mut c_void, jsize, *mut jsize) -> jint;
    let get_vms: GetVMs = std::mem::transmute(p as *const ());

    let mut buf: [*mut c_void; 4] = [std::ptr::null_mut(); 4];
    let mut n: jsize = 0;
    let rc = get_vms(buf.as_mut_ptr(), 4, &mut n);
    if rc != 0 || n < 1 || buf[0].is_null() {
        return Err(format!("JNI_GetCreatedJavaVMs rc={} n={}", rc, n));
    }

    let vm = JavaVM::from_raw(buf[0] as *mut jni::sys::JavaVM)
        .map_err(|e| format!("JavaVM::from_raw 失败: {}", e))?;

    VM_PTR.store(buf[0] as isize, Ordering::SeqCst);
    VM.set(vm).map_err(|_| "VM 已初始化".to_string())?;

    logger::info("JavaVM 获取成功");
    Ok(())
}

pub fn has_vm() -> bool {
    VM.get().is_some()
}

// ─────────────────── 反向注册 ───────────────────

#[no_mangle]
pub extern "system" fn Java_com_fastnow_weaverift_NativeBridge_registerBridge(
    env: *mut RawJNIEnv,
    _class: JClass,
    bridge_class: JClass,
) {
    unsafe {
        let mut jni_env = match JNIEnv::from_raw(env) {
            Ok(e) => e,
            Err(_) => {
                logger::error("registerBridge: JNIEnv::from_raw 失败");
                return;
            }
        };

        // 转 GlobalRef 防止 GC
        let global = match jni_env.new_global_ref(&bridge_class) {
            Ok(g) => g,
            Err(e) => {
                logger::error(&format!("registerBridge: new_global_ref 失败: {}", e));
                return;
            }
        };
        let global_jclass = global.as_raw() as isize;

        // 缓存方法 ID
        let snap_mid = jni_env
            .get_static_method_id(&bridge_class, "snapshotArray", "()[D")
            .map(|m| m.into_raw() as isize)
            .unwrap_or(0);
        let cmd_mid = jni_env
            .get_static_method_id(
                &bridge_class,
                "onCommand",
                "(Ljava/lang/String;)Ljava/lang/String;",
            )
            .map(|m| m.into_raw() as isize)
            .unwrap_or(0);

        // ★ 用 Box::leak 让 GlobalRef 活到进程结束
        let _leaked: &'static _ = Box::leak(Box::new(global));

        BRIDGE_CLASS.store(global_jclass, Ordering::SeqCst);
        SNAPSHOT_MID.store(snap_mid, Ordering::SeqCst);
        COMMAND_MID.store(cmd_mid, Ordering::SeqCst);

        logger::info("RiftBridge 已注册到 native");
    }
}

pub fn is_registered() -> bool {
    BRIDGE_CLASS.load(Ordering::SeqCst) != 0
}

// ─────────────────── 快照 ───────────────────

#[derive(Clone, Copy, Default, Debug)]
pub struct Snapshot {
    pub valid: bool,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub health: f32,
    pub max_health: f32,
    pub entities: i32,
}

impl Snapshot {
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"valid":{},"x":{:.2},"y":{:.2},"z":{:.2},"yaw":{:.1},"pitch":{:.1},"hp":{:.1},"maxHp":{:.1},"ents":{}}}"#,
            self.valid, self.x, self.y, self.z, self.yaw, self.pitch,
            self.health, self.max_health, self.entities
        )
    }
}

pub fn read_snapshot() -> Option<Snapshot> {
    if !is_registered() {
        return None;
    }

    let vm = VM.get()?;
    let mut env = vm.get_env().ok()?;

    let raw_class = BRIDGE_CLASS.load(Ordering::SeqCst) as *mut c_void;
    let raw_mid = SNAPSHOT_MID.load(Ordering::SeqCst) as *mut c_void;

    if raw_class.is_null() || raw_mid.is_null() {
        return None;
    }

    let class = unsafe { JClass::from_raw(raw_class as *mut _) };
    let mid = unsafe { JStaticMethodID::from_raw(raw_mid as *mut _) };

    // ★ 用 jni-rs 的 call_static_method（非 unchecked，避开 Desc 泛型问题）
    let result = unsafe {
        env.call_static_method_unchecked(
            &class,
            mid,
            ReturnType::Array,
            &[],
        )
    };

    let arr = match result {
        Ok(v) => match v.l() {
            Ok(a) => a,
            Err(e) => {
                let _ = env.exception_clear();
                logger::trace(&format!("snapshot l() 失败: {}", e));
                return None;
            }
        },
        Err(e) => {
            let _ = env.exception_clear();
            logger::trace(&format!("snapshotArray 调用失败: {}", e));
            return None;
        }
    };

    let darr = jni::objects::JDoubleArray::from(arr);
    let mut buf: [jdouble; 9] = [0.0; 9];
    if let Err(e) = env.get_double_array_region(&darr, 0, &mut buf) {
        let _ = env.exception_clear();
        logger::trace(&format!("get_double_array_region 失败: {}", e));
        return None;
    }

    Some(Snapshot {
        valid: buf[0] > 0.5,
        x: buf[1],
        y: buf[2],
        z: buf[3],
        yaw: buf[4] as f32,
        pitch: buf[5] as f32,
        health: buf[6] as f32,
        max_health: buf[7] as f32,
        entities: buf[8] as i32,
    })
}

// ─────────────────── 反向命令 ───────────────────

pub fn send_command(cmd: &str) -> Result<String, String> {
    let vm = VM.get().ok_or("VM 未就绪")?;
    let mut env = vm.get_env().map_err(|e| format!("{}", e))?;

    let raw_class = BRIDGE_CLASS.load(Ordering::SeqCst) as *mut c_void;
    let raw_mid = COMMAND_MID.load(Ordering::SeqCst) as *mut c_void;

    if raw_class.is_null() || raw_mid.is_null() {
        return Err("RiftBridge 未注册".into());
    }

    let class = unsafe { JClass::from_raw(raw_class as *mut _) };
    let mid = unsafe { JStaticMethodID::from_raw(raw_mid as *mut _) };

    let arg = env.new_string(cmd).map_err(|e| format!("{}", e))?;

    let result = unsafe {
        env.call_static_method_unchecked(
            &class,
            mid,
            ReturnType::Object,
            &[JValue::Object(&arg).as_jni()],
        )
    };

    let ret = result.map_err(|e| {
        let _ = env.exception_clear();
        format!("{}", e)
    })?;

    let s = ret.l().map_err(|e| format!("{}", e))?;
    let js: jni::objects::JString = s.into();
    let out = env.get_string(&js)
        .map(|x| x.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(out)
}

// ─────────────────── 采样节流 ───────────────────

pub struct Sampler {
    period_ms: u128,
    last: Option<Instant>,
    cached: Option<Snapshot>,
}

pub fn get_vm() -> Option<&'static JavaVM> {
    VM.get()
}

impl Sampler {
    pub fn new(hz: u32) -> Self {
        Self {
            period_ms: 1000 / hz.max(1) as u128,
            last: None,
            cached: None,
        }
    }
    pub fn poll(&mut self) -> Option<Snapshot> {
        let now = Instant::now();
        let due = self
            .last
            .map(|t| now.duration_since(t).as_millis() >= self.period_ms)
            .unwrap_or(true);
        if due {
            self.last = Some(now);
            if let Some(s) = read_snapshot() {
                self.cached = Some(s);
            }
        }
        self.cached
    }
}