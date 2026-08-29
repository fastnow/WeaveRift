use std::ffi::c_void;
use std::mem;
use std::ptr;
use windows::Win32::Foundation::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::core::{PCSTR, PCWSTR};
use jni::objects::{JValue, JObject};
use jni::sys::jint;
use jni::JavaVM;

#[repr(C)]
struct SharedPath {
    magic: u32,
    mode: u32,      // 0 = classpath load, 1 = agent load
    path: [u16; 260],
}

const SHARED_NAME: &str = "Global\\WeaveRift.Path";
const MAGIC: u32 = 0x57415645;

fn read_shared_path() -> Option<(String, u32)> {
    let name_wide: Vec<u16> = SHARED_NAME.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe {
        OpenFileMappingW(FILE_MAP_ALL_ACCESS.0, BOOL(0), PCWSTR(name_wide.as_ptr()))
    };
    let h = match handle {
        Ok(h) => h,
        Err(_) => return None,
    };

    let view = unsafe { MapViewOfFile(h, FILE_MAP_ALL_ACCESS, 0, 0, mem::size_of::<SharedPath>()) };
    if view.Value.is_null() {
        unsafe { CloseHandle(h) };
        return None;
    }

    let shared = view.Value as *const SharedPath;
    let magic = unsafe { (*shared).magic };
    if magic != MAGIC {
        unsafe { UnmapViewOfFile(view); CloseHandle(h) };
        return None;
    }

    let mode = unsafe { (*shared).mode };
    let path_buf = unsafe { (*shared).path };
    let len = path_buf.iter().position(|&c| c == 0).unwrap_or(260);
    let path = String::from_utf16_lossy(&path_buf[..len]);
    unsafe { UnmapViewOfFile(view); CloseHandle(h) };
    Some((path, mode))
}

// ─── 加载 Java Agent（通过 VirtualMachine.loadAgent）───
fn load_java_agent(jar_path: &str) -> Result<(), String> {
    type JniGetCreatedVmsFn = unsafe extern "system" fn(
        vm_buf: *mut *mut jni::sys::JavaVM,
        buf_len: jint,
        n_vms: *mut jint,
    ) -> jint;

    let get_vms: JniGetCreatedVmsFn = {
        let name: Vec<u16> = "jvm.dll\0".encode_utf16().collect();
        let hmod = unsafe { GetModuleHandleW(PCWSTR(name.as_ptr())) }
            .map_err(|_| "jvm.dll not loaded".to_string())?;
        let fptr = unsafe { GetProcAddress(hmod, PCSTR("JNI_GetCreatedJavaVMs\0".as_ptr())) }
            .ok_or("JNI_GetCreatedJavaVMs not found")?;
        unsafe { std::mem::transmute(fptr) }
    };

    let mut vm_ptr: *mut jni::sys::JavaVM = ptr::null_mut();
    let mut count: jint = 0;
    let result = unsafe { get_vms(&mut vm_ptr, 1, &mut count) };
    if result != 0 || count == 0 || vm_ptr.is_null() {
        return Err("No Java VM found".into());
    }

    let vm = unsafe { JavaVM::from_raw(vm_ptr) }.map_err(|_| "JavaVM::from_raw failed")?;
    let mut env = vm.attach_current_thread().map_err(|_| "AttachCurrentThread failed")?;  // <-- 加了 mut

    // 1. 查找 VirtualMachine 类
    let vm_cls = env.find_class("com/sun/tools/attach/VirtualMachine")
        .map_err(|e| format!("VirtualMachine class not found: {}", e))?;

    // 2. 调用 VirtualMachine.attach(pid)
    let pid_str = env.new_string(&std::process::id().to_string())
        .map_err(|e| format!("new_string failed: {}", e))?;
    let vm_obj = env.call_static_method(
        &vm_cls,
        "attach",
        "(Ljava/lang/String;)Lcom/sun/tools/attach/VirtualMachine;",
        &[JValue::from(&*pid_str)],
    ).map_err(|e| format!("VirtualMachine.attach failed: {}", e))?;
    let vm_obj = vm_obj.l().map_err(|_| "attach returned null")?;

    // 3. 调用 vm.loadAgent(jar_path)
    let path_str = env.new_string(jar_path)
        .map_err(|e| format!("new_string failed: {}", e))?;
    env.call_method(
        vm_obj,
        "loadAgent",
        "(Ljava/lang/String;)V",
        &[JValue::from(&*path_str)],
    ).map_err(|e| format!("loadAgent failed: {}", e))?;

    println!("[WeaveRift] Java Agent loaded successfully: {}", jar_path);
    Ok(())
}

// ─── 原有：通过 URLClassLoader 加载 JAR ───
fn load_jar_into_classpath(jar_path: &str) -> Result<(), String> {
    type JniGetCreatedVmsFn = unsafe extern "system" fn(
        vm_buf: *mut *mut jni::sys::JavaVM,
        buf_len: jint,
        n_vms: *mut jint,
    ) -> jint;

    let get_vms: JniGetCreatedVmsFn = {
        let name: Vec<u16> = "jvm.dll\0".encode_utf16().collect();
        let hmod = unsafe { GetModuleHandleW(PCWSTR(name.as_ptr())) }
            .map_err(|_| "jvm.dll not loaded".to_string())?;
        let fptr = unsafe { GetProcAddress(hmod, PCSTR("JNI_GetCreatedJavaVMs\0".as_ptr())) }
            .ok_or("JNI_GetCreatedJavaVMs not found")?;
        unsafe { std::mem::transmute(fptr) }
    };

    let mut vm_ptr: *mut jni::sys::JavaVM = ptr::null_mut();
    let mut count: jint = 0;
    let result = unsafe { get_vms(&mut vm_ptr, 1, &mut count) };
    if result != 0 || count == 0 || vm_ptr.is_null() {
        return Err("No Java VM found".into());
    }

    let vm = unsafe { JavaVM::from_raw(vm_ptr) }.map_err(|_| "JavaVM::from_raw failed")?;
    let mut env = vm.attach_current_thread().map_err(|_| "AttachCurrentThread failed")?;  // <-- 加了 mut

    // 1. 查找类
    let class_loader_cls = env.find_class("java/net/URLClassLoader")
        .map_err(|e| e.to_string())?;
    let url_cls = env.find_class("java/net/URL")
        .map_err(|e| e.to_string())?;

    // 2. 构造 file:// URL
    let path_abs = std::fs::canonicalize(jar_path).map_err(|e| e.to_string())?;
    let url_str = format!("file:///{}", path_abs.to_string_lossy().replace('\\', "/"));
    let url_obj = env.new_string(&url_str).map_err(|e| e.to_string())?;
    let url_instance = env.new_object(
        &url_cls,
        "(Ljava/lang/String;)V",
        &[JValue::from(&*url_obj)],
    ).map_err(|e| e.to_string())?;

    // 3. 获取系统类加载器
    let sys_loader = env.call_static_method(
        "java/lang/ClassLoader",
        "getSystemClassLoader",
        "()Ljava/lang/ClassLoader;",
        &[],
    ).map_err(|e| e.to_string())?;
    let sys_loader = sys_loader.l().map_err(|_| "Invalid system loader")?;

    // 4. 尝试 addURL（直接加到系统加载器）
    if env.get_method_id(&class_loader_cls, "addURL", "(Ljava/net/URL;)V").is_ok() {
        env.call_method(
            sys_loader,
            "addURL",
            "(Ljava/net/URL;)V",
            &[JValue::from(&url_instance)],
        ).map_err(|e| e.to_string())?;
    } else {
        // 回退：新建 URLClassLoader 并设为上下文类加载器
        let url_array = env.new_object_array(1, &url_cls, &url_instance)
            .map_err(|e| e.to_string())?;
        let new_loader = env.call_static_method(
            &class_loader_cls,
            "newInstance",
            "([Ljava/net/URL;Ljava/lang/ClassLoader;)Ljava/net/URLClassLoader;",
            &[JValue::from(&url_array), JValue::from(&sys_loader)],
        ).map_err(|e| e.to_string())?;
        let new_loader = new_loader.l().map_err(|_| "Invalid new loader")?;

        let thread_cls = env.find_class("java/lang/Thread").map_err(|e| e.to_string())?;
        let current = env.call_static_method(
            &thread_cls,
            "currentThread",
            "()Ljava/lang/Thread;",
            &[],
        ).map_err(|e| e.to_string())?;
        let current = current.l().map_err(|_| "Invalid current thread")?;
        env.call_method(
            current,
            "setContextClassLoader",
            "(Ljava/lang/ClassLoader;)V",
            &[JValue::from(&new_loader)],
        ).map_err(|e| e.to_string())?;
    }

    Ok(())
}

// ─── DLL 入口 ───
#[no_mangle]
pub extern "system" fn DllMain(_hinst: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == 1 {
        unsafe { module_hide::hide_module(_hinst.0) };
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(300));
            if let Some((path, mode)) = read_shared_path() {
                match mode {
                    1 => { let _ = load_java_agent(&path); }
                    _ => { let _ = load_jar_into_classpath(&path); }
                }
            }
        });
    }
    TRUE.into()
}

#[no_mangle]
pub extern "system" fn ReflectiveLoader(lp: *mut c_void) -> u32 {
    if !lp.is_null() {
        unsafe { module_hide::hide_module(lp) };
    }
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(300));
        if let Some((path, mode)) = read_shared_path() {
            match mode {
                1 => { let _ = load_java_agent(&path); }
                _ => { let _ = load_jar_into_classpath(&path); }
            }
        }
    });
    0
}