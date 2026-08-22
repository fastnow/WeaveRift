fn main() {
    #[cfg(target_os = "windows")]
    {
        use winres::WindowsResource;
        let mut res = WindowsResource::new();
        if std::path::Path::new("icon.ico").exists() {
            res.set_icon("icon.ico");
        }
        res.set_language(0x0804);
        res.set("FileDescription", "FlashDllInjector - Windows DLL Injection Tool");
        res.set("ProductName", "FlashDllInjector");
        res.set("OriginalFilename", "FlashDllInjector.exe");
        res.set("CompanyName", "FastNow Studio");
        res.set("LegalCopyright", "Copyright (c) 2026 FastNow Studio");
        res.compile().expect("Failed to compile Windows resources");
    }
}