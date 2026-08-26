use std::ffi::c_void;
use std::mem;
use std::ptr;
use windows::Win32::Foundation::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::core::{PCSTR, PCWSTR};
use jni::objects::JValue;
use jni::sys::jint;
use jni::JavaVM;

#[repr(C)]
struct SharedPath {
    magic: u32,
    path: [u16; 260],
}

const SHARED_NAME: &str = "Global\\FlashJarLoader.Path";

fn read_jar_path() -> Option<String> {
    let name_wide: Vec<u16> = SHARED_NAME.encode_utf16().chain(Some(0)).collect();
    // 本 DLL 是“读取方”：打开已存在的共享内存，不存在则直接返回 None
    let handle = unsafe {
        OpenFileMappingW(FILE_MAP_ALL_ACCESS.0, BOOL(0), windows::core::PCWSTR(name_wide.as_ptr()))
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
    if magic != 0x4A4152 {
        unsafe { UnmapViewOfFile(view); CloseHandle(h) };
        return None;
    }

    let path_buf = unsafe { (*shared).path };
    let len = path_buf.iter().position(|&c| c == 0).unwrap_or(260);
    let path = String::from_utf16_lossy(&path_buf[..len]);
    unsafe { UnmapViewOfFile(view); CloseHandle(h) };
    Some(path)
}

fn load_jar_into_jvm(jar_path: &str) -> Result<(), String> {
    // 本 DLL 被注入进 JVM 进程，jvm.dll 已加载。JNI_GetCreatedJavaVMs 是 jvm.dll 的
    // 运行时导出，不能静态链接，这里用 GetProcAddress 动态解析。
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
    let mut env = vm.attach_current_thread().map_err(|_| "AttachCurrentThread failed")?;

    let class_loader_cls = env.find_class("java/net/URLClassLoader").map_err(|e| e.to_string())?;
    let url_cls = env.find_class("java/net/URL").map_err(|e| e.to_string())?;

    let path_abs = std::fs::canonicalize(jar_path).map_err(|e| e.to_string())?;
    let url_str = format!("file:///{}", path_abs.to_string_lossy().replace('\\', "/"));
    let url_obj = env.new_string(&url_str).map_err(|e| e.to_string())?;
    // jni 0.21：new_object 第二参为构造签名（"<init>" 由它推导）
    let url_instance = env.new_object(&url_cls, "(Ljava/lang/String;)V", &[(&url_obj).into()])
        .map_err(|e| e.to_string())?;

    let sys_cls = env.find_class("java/lang/ClassLoader").map_err(|e| e.to_string())?;
    let sys_loader = env.call_static_method(
        sys_cls,
        "getSystemClassLoader",
        "()Ljava/lang/ClassLoader;",
        &[],
    ).map_err(|e| e.to_string())?;
    let loader_obj = sys_loader.l().map_err(|_| "Invalid system loader")?;

    // 优先把 URL 加到系统类加载器（JNI 绕过访问控制，addURL 可直接调用）
    if env.get_method_id(&class_loader_cls, "addURL", "(Ljava/net/URL;)V").is_ok() {
        env.call_method(loader_obj, "addURL", "(Ljava/net/URL;)V", &[(&url_instance).into()])
            .map_err(|e| e.to_string())?;
    } else {
        // 回退：新建 URLClassLoader 并设为当前线程上下文类加载器
        let url_array = env.new_object_array(1, &url_cls, &url_instance)
            .map_err(|e| e.to_string())?;
        let new_loader = env.call_static_method(
            &class_loader_cls,
            "newInstance",
            "([Ljava/net/URL;Ljava/lang/ClassLoader;)Ljava/net/URLClassLoader;",
            &[JValue::Object(&*url_array), JValue::Object(&loader_obj)],
        ).map_err(|e| e.to_string())?;
        let new_loader_obj = new_loader.l().map_err(|_| "Invalid new loader")?;

        let thread_cls = env.find_class("java/lang/Thread").map_err(|e| e.to_string())?;
        let current = env.call_static_method(
            thread_cls,
            "currentThread",
            "()Ljava/lang/Thread;",
            &[],
        ).map_err(|e| e.to_string())?;
        let current_obj = current.l().map_err(|_| "Invalid current thread")?;
        env.call_method(current_obj, "setContextClassLoader", "(Ljava/lang/ClassLoader;)V", &[JValue::Object(&new_loader_obj)])
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[no_mangle]
pub extern "system" fn DllMain(_hinst: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == 1 {
        // 反作弊测试：把自己从 PEB 加载链表中摘除
        unsafe { module_hide::hide_module(_hinst.0) };
        std::thread::spawn(|| {
            if let Some(path) = read_jar_path() {
                let _ = load_jar_into_jvm(&path);
            }
        });
    }
    TRUE.into()
}

#[no_mangle]
pub extern "system" fn ReflectiveLoader(lp: *mut c_void) -> u32 {
    // Reflective 注入时，lp 是被注入 DLL 自身的镜像基址（可用作隐藏对象）
    if !lp.is_null() {
        unsafe { module_hide::hide_module(lp) };
    }
    std::thread::spawn(|| {
        if let Some(path) = read_jar_path() {
            let _ = load_jar_into_jvm(&path);
        }
    });
    0
}
