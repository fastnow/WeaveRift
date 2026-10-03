// jar_loader/build.rs
fn main() {
    cc::Build::new()
        .file("src/hook_entry.c")
        .flag("/EHsc")
        .flag("/O2")
        .compile("hook_entry");

    println!("cargo:rerun-if-changed=src/hook_entry.c");
}