use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicPtr, Ordering};
use windows::Win32::Foundation::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::LibraryLoader::{
    GetModuleHandleW, GetProcAddress, DisableThreadLibraryCalls,
};
use windows::core::{PCSTR, PCWSTR};
use jni::objects::{JValue, JString, JByteArray};
use jni::sys::{jint, jclass, jstring, jbyteArray};
use jni::JavaVM;
use jvmti_bindings::env::Jvmti;

#[repr(C)]
struct SharedPath {
    magic: u32,
    mode: u32,
    target_pid: u32,
    jar_path: [u16; 260],
}

const SHARED_NAME: &str = "Local\\WeaveRift.Path";
const MAGIC: u32 = 0x57415645;

static G_JVMTI: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

fn find_srg_path(jar_path: &str) -> Option<PathBuf> {
    let jar = PathBuf::from(jar_path);
    let dir = jar.parent()?.to_path_buf();
    let candidates = [
        dir.join("obf2srg.srg"),
        dir.join("mappings").join("obf2srg.srg"),
    ];
    for c in &candidates {
        if c.exists() { return Some(c.clone()); }
    }
    None
}

fn read_shared_path() -> Option<(String, u32)> {
    let name_wide: Vec<u16> = SHARED_NAME.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe {
        OpenFileMappingW(FILE_MAP_ALL_ACCESS.0, BOOL(0), PCWSTR(name_wide.as_ptr()))
    };
    let h = match handle { Ok(h) => h, Err(_) => return None };

    let view = unsafe { MapViewOfFile(h, FILE_MAP_ALL_ACCESS, 0, 0, mem::size_of::<SharedPath>()) };
    if view.Value.is_null() {
        unsafe { let _ = CloseHandle(h); }
        return None;
    }

    let shared = view.Value as *const SharedPath;
    if unsafe { (*shared).magic } != MAGIC {
        unsafe { let _ = UnmapViewOfFile(view); let _ = CloseHandle(h); }
        return None;
    }

    let mode = unsafe { (*shared).mode };
    let path_buf = unsafe { (*shared).jar_path };
    let len = path_buf.iter().position(|&c| c == 0).unwrap_or(260);
    let path = String::from_utf16_lossy(&path_buf[..len]);
    unsafe { let _ = UnmapViewOfFile(view); let _ = CloseHandle(h); }
    Some((path, mode))
}

fn find_jvm() -> Result<*mut jni::sys::JavaVM, String> {
    type JniGetCreatedVmsFn = unsafe extern "system" fn(
        vm_buf: *mut *mut jni::sys::JavaVM,
        buf_len: jint,
        n_vms: *mut jint,
    ) -> jint;

    let name: Vec<u16> = "jvm.dll\0".encode_utf16().collect();
    let hmod = unsafe { GetModuleHandleW(PCWSTR(name.as_ptr())) }
        .map_err(|_| "jvm.dll not loaded".to_string())?;
    let fptr = unsafe { GetProcAddress(hmod, PCSTR("JNI_GetCreatedJavaVMs\0".as_ptr())) }
        .ok_or("JNI_GetCreatedJavaVMs not found")?;
    let get_vms: JniGetCreatedVmsFn = unsafe { mem::transmute(fptr) };

    let mut vm_ptr: *mut jni::sys::JavaVM = ptr::null_mut();
    let mut count: jint = 0;
    if unsafe { get_vms(&mut vm_ptr, 1, &mut count) } != 0 || count == 0 {
        return Err("No Java VM found".into());
    }
    Ok(vm_ptr)
}

fn load_agent_via_attach(env: &mut jni::JNIEnv, jar_path: &str, srg_path: Option<&str>) -> Result<(), String> {
    let abs_path = std::path::Path::new(jar_path);
    let abs_path = if abs_path.is_absolute() { abs_path.to_path_buf() }
        else { std::env::current_dir().map_err(|e| e.to_string())?.join(abs_path) };
    let jar_str = abs_path.to_string_lossy().to_string();

    let pid = std::process::id().to_string();
    let vm_cls = env.find_class("com/sun/tools/attach/VirtualMachine")
        .map_err(|e| format!("VirtualMachine not found: {}", e))?;
    let pid_jstr = env.new_string(&pid).map_err(|e| e.to_string())?;
    let vm_obj = env.call_static_method(&vm_cls, "attach",
        "(Ljava/lang/String;)Lcom/sun/tools/attach/VirtualMachine;",
        &[JValue::from(&pid_jstr)])
        .map_err(|e| format!("attach failed: {}", e))?
        .l().map_err(|_| "attach returned null")?;

    let sys_cls = env.find_class("java/lang/System").map_err(|e| e.to_string())?;

    // ─── 设置 weaverift.srg ───────────────────
    if let Some(srg) = srg_path {
        let key = env.new_string("weaverift.srg").map_err(|e| e.to_string())?;
        let val = env.new_string(srg).map_err(|e| e.to_string())?;
        let _ = env.call_static_method(&sys_cls, "setProperty",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::from(&key), JValue::from(&val)]);
        println!("[WeaveRift] Set weaverift.srg = {}", srg);
    }

    // ─── 设置 weaverift.dll ────────────────────
    let jar_pb = PathBuf::from(jar_path);
    if let Some(dir) = jar_pb.parent() {
        let dll_path = dir.join("jar_loader.dll");
        if dll_path.exists() {
            let dll_str = dll_path.to_string_lossy().to_string();
            let key = env.new_string("weaverift.dll").map_err(|e| e.to_string())?;
            let val = env.new_string(&dll_str).map_err(|e| e.to_string())?;
            let _ = env.call_static_method(&sys_cls, "setProperty",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::from(&key), JValue::from(&val)]);
            println!("[WeaveRift] Set weaverift.dll = {}", dll_str);
        } else {
            println!("[WeaveRift] jar_loader.dll not found next to JAR: {}", dll_path.display());
        }
    }

    let jar_jstr = env.new_string(&jar_str).map_err(|e| e.to_string())?;
    env.call_method(&vm_obj, "loadAgent", "(Ljava/lang/String;)V",
        &[JValue::from(&jar_jstr)]).map_err(|e| format!("loadAgent failed: {}", e))?;
    let _ = env.call_method(&vm_obj, "detach", "()V", &[]);

    println!("[WeaveRift] Agent loaded: {}", jar_str);
    Ok(())
}

fn worker_thread() {
    std::thread::sleep(std::time::Duration::from_millis(500));

    let Some((path, mode)) = read_shared_path() else {
        eprintln!("[WeaveRift] No shared data found");
        return;
    };
    println!("[WeaveRift] Shared JAR: {}", path);

    let srg_path = find_srg_path(&path);
    let srg_str = srg_path.as_ref().map(|p| p.to_string_lossy().to_string());

    let vm_ptr = match find_jvm() {
        Ok(p) => p,
        Err(e) => { eprintln!("[WeaveRift] {}", e); return }
    };

    let vm = unsafe { JavaVM::from_raw(vm_ptr) }.unwrap();
    let mut env = match vm.attach_current_thread_as_daemon() {
        Ok(e) => e,
        Err(e) => { eprintln!("[WeaveRift] Attach failed: {}", e); return }
    };

    if mode == 0 {
        if let Err(e) = load_agent_via_attach(&mut env, &path, srg_str.as_deref()) {
            eprintln!("[WeaveRift] {}", e);
        }
    }

    unsafe { let _ = vm.detach_current_thread(); }
}

// ─── JNI_OnLoad ──────────────────────────────────────────────

#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: *mut jni::sys::JavaVM, _reserved: *mut c_void) -> jint {
    println!("[WeaveRift] JNI_OnLoad called");

    unsafe {
        let get_env: extern "system" fn(*mut jni::sys::JavaVM, *mut *mut c_void, jint) -> jint =
            mem::transmute((*(*vm)).GetEnv);
        let mut jvmti_ptr: *mut c_void = ptr::null_mut();
        let rc = get_env(vm, &mut jvmti_ptr, 0x30010200);
        if rc == 0 && !jvmti_ptr.is_null() {
            G_JVMTI.store(jvmti_ptr, Ordering::Release);
            println!("[WeaveRift] JVMTI env obtained: {:?}", jvmti_ptr);

            let jvmti = Jvmti::from_raw(jvmti_ptr as *mut jvmti_bindings::sys::jvmti::jvmtiEnv);

            let mut caps = jvmti_bindings::sys::jvmti::jvmtiCapabilities::default();
            caps.set_can_redefine_classes(true);
            caps.set_can_retransform_classes(true);

            match jvmti.add_capabilities(&caps) {
                Ok(_) => println!("[WeaveRift] Capabilities added (redefine)"),
                Err(e) => println!("[WeaveRift] AddCapabilities failed: {:?}", e),
            }
        } else {
            println!("[WeaveRift] GetEnv for JVMTI failed: {}", rc);
        }
    }

    jni::sys::JNI_VERSION_1_8
}

// ─── redefineClass ───────────────────────────────────────────

#[no_mangle]
pub extern "system" fn Java_com_fastnow_weaverift_NativeBridge_redefineClass(
    env: *mut jni::sys::JNIEnv,
    _class: jclass,
    class_name: jstring,
    new_bytes: jbyteArray,
) -> jint {
    println!("[WeaveRift] redefineClass called");

    let jvmti_ptr = G_JVMTI.load(Ordering::Acquire);
    if jvmti_ptr.is_null() {
        println!("[WeaveRift] JVMTI not initialized");
        return -1;
    }

    let mut jni_env = unsafe { jni::JNIEnv::from_raw(env).unwrap() };

    let class_name_obj = unsafe { JString::from_raw(class_name) };
    let class_name_str = match jni_env.get_string(&class_name_obj) {
        Ok(s) => s.to_string_lossy().to_string(),
        Err(e) => {
            println!("[WeaveRift] get_string failed: {}", e);
            return -1;
        }
    };
    println!("[WeaveRift] Target class: {}", class_name_str);

    let bytes_obj = unsafe { JByteArray::from_raw(new_bytes) };
    let bytes = match jni_env.convert_byte_array(&bytes_obj) {
        Ok(b) => b,
        Err(e) => {
            println!("[WeaveRift] convert_byte_array failed: {}", e);
            return -1;
        }
    };
    println!("[WeaveRift] New class bytes: {} bytes", bytes.len());

    let jvmti = unsafe { Jvmti::from_raw(jvmti_ptr as *mut jvmti_bindings::sys::jvmti::jvmtiEnv) };

    let class_name_slash = class_name_str.replace('.', "/");
    let class_name_l = format!("L{};", class_name_slash);

    let classes = match unsafe { jvmti.get_loaded_classes() } {
        Ok(c) => c,
        Err(e) => {
            println!("[WeaveRift] get_loaded_classes failed: {:?}", e);
            return -1;
        }
    };
    println!("[WeaveRift] Loaded classes: {}", classes.len());

    let mut target_class: Option<jvmti_bindings::sys::jni::jclass> = None;
    for cls in &classes {
        if let Ok((sig, _generic)) = unsafe { jvmti.get_class_signature(*cls) } {
            if sig == class_name_l || sig == class_name_slash {
                target_class = Some(*cls);
                break;
            }
        }
    }

    let target_class = match target_class {
        Some(c) => c,
        None => {
            println!("[WeaveRift] Class not found: {}", class_name_str);
            return -1;
        }
    };
    println!("[WeaveRift] Found target class");

    let class_def = jvmti_bindings::sys::jvmti::jvmtiClassDefinition {
        klass: target_class,
        class_byte_count: bytes.len() as i32,
        class_bytes: bytes.as_ptr() as *const u8,
    };

    match unsafe { jvmti.redefine_classes(&[class_def]) } {
        Ok(_) => {
            println!("[WeaveRift] RedefineClasses succeeded");
            0
        }
        Err(e) => {
            println!("[WeaveRift] RedefineClasses failed: {:?}", e);
            -1
        }
    }
}

// ─── DllMain ─────────────────────────────────────────────────

#[no_mangle]
pub extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    if reason == 1 {
        let _ = unsafe { DisableThreadLibraryCalls(hinst) };
        unsafe { let _ = module_hide::hide_module(hinst.0); }
        std::thread::spawn(worker_thread);
    }
    BOOL(1)
}

#[no_mangle]
pub extern "system" fn ReflectiveLoader(lp: *mut c_void) -> u32 {
    if !lp.is_null() {
        unsafe { let _ = module_hide::hide_module(lp); }
    }
    std::thread::spawn(worker_thread);
    0
}