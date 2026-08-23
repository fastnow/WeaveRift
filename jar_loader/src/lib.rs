use std::ffi::c_void;
use std::mem;
use windows::Win32::Foundation::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Threading::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::{jint, jlong};
use jni::JNIEnv;
use std::ptr;

#[repr(C, packed)]
struct SharedPath {
    magic: u32,
    path: [u16; 260],
}

const SHARED_NAME: &str = "Global\\FlashJarLoader.Path";

fn read_jar_path() -> Option<String> {
    let name_wide: Vec<u16> = SHARED_NAME.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe {
        CreateFileMappingW(
            HANDLE::default(),
            None,
            PAGE_READWRITE,
            0,
            mem::size_of::<SharedPath>() as u32,
            windows::core::PCWSTR(name_wide.as_ptr()),
        )
    };
    if let Ok(h) = handle {
        let view = unsafe {
            MapViewOfFile(h, FILE_MAP_ALL_ACCESS, 0, 0, mem::size_of::<SharedPath>())
        };
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
        let path_wide = unsafe { &(*shared).path };
        let len = path_wide.iter().position(|&c| c == 0).unwrap_or(260);
        let path = String::from_utf16_lossy(&path_wide[..len]);
        unsafe { UnmapViewOfFile(view); CloseHandle(h) };
        Some(path)
    } else {
        None
    }
}

fn load_jar_into_jvm(jar_path: &str) -> Result<(), String> {
    let mut vm_ptr = ptr::null_mut();
    let mut count = 0;
    let result = unsafe {
        jni::JNI_GetCreatedJavaVMs(&mut vm_ptr, 1, &mut count)
    };
    if result != 0 || count == 0 {
        return Err("No Java VM found".into());
    }

    let vm = vm_ptr;
    let mut env_ptr = ptr::null_mut();
    let attach_result = unsafe {
        (*(vm as *mut jni::sys::JavaVM)).AttachCurrentThread(&mut env_ptr, ptr::null_mut())
    };
    if attach_result != 0 || env_ptr.is_null() {
        return Err("AttachCurrentThread failed".into());
    }
    let env = JNIEnv::from_raw(env_ptr).map_err(|_| "JNIEnv creation failed")?;

    let class_loader_cls = env.find_class("java/net/URLClassLoader").map_err(|e| e.to_string())?;
    let url_cls = env.find_class("java/net/URL").map_err(|e| e.to_string())?;

    let path_abs = std::fs::canonicalize(jar_path).map_err(|e| e.to_string())?;
    let url_str = format!("file:///{}", path_abs.to_string_lossy().replace('\\', "/"));
    let url_obj = env.new_string(&url_str).map_err(|e| e.to_string())?;
    let url_constructor = env.get_method_id(url_cls, "<init>", "(Ljava/lang/String;)V")
        .map_err(|e| e.to_string())?;
    let url_instance = env.new_object(url_cls, url_constructor, &[JValue::from(url_obj)])
        .map_err(|e| e.to_string())?;

    let sys_cls = env.find_class("java/lang/ClassLoader").map_err(|e| e.to_string())?;
    let get_sys = env.get_static_method_id(sys_cls, "getSystemClassLoader", "()Ljava/lang/ClassLoader;")
        .map_err(|e| e.to_string())?;
    let sys_loader = env.call_static_method(sys_cls, get_sys, &[]).map_err(|e| e.to_string())?;
    let loader_obj = sys_loader.l().map_err(|_| "Invalid system loader")?;

    let add_url_mid = env.get_method_id(class_loader_cls, "addURL", "(Ljava/net/URL;)V")
        .or_else(|_| env.get_method_id(class_loader_cls, "addURL", "(Ljava/net/URL;)V"));
    match add_url_mid {
        Ok(mid) => {
            env.call_method(loader_obj, mid, &[JValue::from(url_instance)])
                .map_err(|e| e.to_string())?;
        }
        Err(_) => {
            let new_loader_mid = env.get_method_id(
                class_loader_cls,
                "newInstance",
                "([Ljava/net/URL;Ljava/lang/ClassLoader;)Ljava/net/URLClassLoader;"
            );
            if let Ok(mid) = new_loader_mid {
                let url_array = env.new_object_array(1, url_cls, &[url_instance.into()])
                    .map_err(|e| e.to_string())?;
                let new_loader = env.call_method(class_loader_cls, mid, &[
                    JValue::Object(&url_array),
                    JValue::Object(&loader_obj),
                ]).map_err(|e| e.to_string())?;
                let thread_cls = env.find_class("java/lang/Thread").map_err(|e| e.to_string())?;
                let current_thread_mid = env.get_static_method_id(thread_cls, "currentThread", "()Ljava/lang/Thread;")
                    .map_err(|e| e.to_string())?;
                let current = env.call_static_method(thread_cls, current_thread_mid, &[])
                    .map_err(|e| e.to_string())?;
                let set_context_mid = env.get_method_id(thread_cls, "setContextClassLoader", "(Ljava/lang/ClassLoader;)V")
                    .map_err(|e| e.to_string())?;
                env.call_method(current, set_context_mid, &[JValue::Object(&new_loader)])
                    .map_err(|e| e.to_string())?;
            } else {
                return Err("Cannot find addURL method for URLClassLoader".into());
            }
        }
    }

    Ok(())
}

#[no_mangle]
pub extern "system" fn DllMain(_hinst: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == 1 { // DLL_PROCESS_ATTACH
        std::thread::spawn(|| {
            if let Some(path) = read_jar_path() {
                let _ = load_jar_into_jvm(&path);
            }
        });
    }
    TRUE.into()
}

#[no_mangle]
pub extern "system" fn ReflectiveLoader(_lp: *mut c_void) -> u32 {
    std::thread::spawn(|| {
        if let Some(path) = read_jar_path() {
            let _ = load_jar_into_jvm(&path);
        }
    });
    0
}