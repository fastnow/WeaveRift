// SEH 兜底胶水层。

#include <windows.h>
#include <stdint.h>

// Rust 钩子主体：正常返回 0，异常由 C 层兜
extern int32_t rust_hook_body(void* hdc);

// Rust 侧日志回调
extern void rust_log_exception(uint32_t code, const char* where);

// Rust 侧提供：拿原始 SwapBuffers 函数指针
typedef BOOL (WINAPI *SwapFn)(HDC);
extern SwapFn rust_get_original_swap(void);

__declspec(noinline)
static BOOL call_original_safe(HDC hdc) {
    SwapFn orig = rust_get_original_swap();
    if (orig == NULL) {
        return FALSE;
    }
    return orig(hdc);
}

__declspec(noinline)
BOOL hook_entry(HDC hdc) {
    __try {
        // Rust 钩子主体
        int32_t rc = rust_hook_body((void*)hdc);
        if (rc != 0) {
            rust_log_exception(0xFFFFFFFF, "rust_hook_body returned non-zero");
        }
    }
    __except (EXCEPTION_EXECUTE_HANDLER) {
        DWORD code = GetExceptionCode();
        rust_log_exception((uint32_t)code, "SEH in hook_entry");
    }

    return call_original_safe(hdc);
}