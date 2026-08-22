use FlashDllInjector::injector::{inject, is_process_64bit, InjectMethod, InjectResult};
use FlashDllInjector::process_finder;
use FlashDllInjector::logger;

use inquire::{Select, Confirm};
use std::io::Write;
use std::path::PathBuf;

fn print_status(pid: Option<u32>, method: InjectMethod, dlls: &[PathBuf]) {
    println!("\n┌──────────────────────────────────────────────");
    if let Some(p) = pid {
        println!("│ 🎯 Target: PID {}", p);
    } else {
        println!("│ 🎯 Target: (not selected)");
    }
    println!("│ 💉 Method: {}", method);
    println!("│ 📦 DLLs: {}", if dlls.is_empty() { "(none)".into() } else { dlls.len().to_string() });
    for (i, d) in dlls.iter().enumerate() {
        println!("│    {}. {}", i + 1, d.display());
    }
    println!("│ Copyright © 2026 FastNow Studio | Apache-2.0");
    println!("└──────────────────────────────────────────────\n");
}

fn print_progress(current: usize, total: usize, msg: &str) {
    let pct = (current * 100) / total;
    let filled = (pct * 20) / 100;
    let bar: String = std::iter::repeat('█').take(filled)
        .chain(std::iter::repeat('░').take(20 - filled)).collect();
    print!("\r   [{}] {:>3}% - {}", bar, pct, msg);
    std::io::stdout().flush().ok();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    logger::init_logger().ok();
    logger::info("FlashDllInjector v2.3 started");

    let mut pid: Option<u32> = None;
    let mut method = InjectMethod::LoadLibrary;
    let mut dlls: Vec<PathBuf> = Vec::new();
    let mut procs: Vec<process_finder::ProcessInfo> = Vec::new();

    loop {
        print_status(pid, method, &dlls);

        let opts = vec![
            "🎯 Select Target Process",
            "💉 Select Inject Method",
            "📁 Add DLL(s)",
            "🗑  Remove DLL",
            "🧹 Clear DLL List",
            "🚀 Execute Injection",
            "👋 Exit",
        ];

        match Select::new("Select action:", opts).prompt()? {
            "🎯 Select Target Process" => {
                println!("Scanning...");
                procs = process_finder::find_minecraft_processes();
                if procs.is_empty() {
                    println!("❌ No process found.");
                    continue;
                }
                if procs.len() == 1 {
                    pid = Some(procs[0].pid);
                    println!("✅ Auto-selected PID {} ({})", procs[0].pid, procs[0].name);
                } else {
                    let items: Vec<String> = procs.iter().map(|p| format!("PID: {:6}  {}  ({})", p.pid, p.name, p.title)).collect();
                    let sel = Select::new("Select process:", items).prompt()?;
                    let p = sel.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                    if p != 0 { pid = Some(p); println!("✅ Selected PID {}", p); }
                }
            }
            "💉 Select Inject Method" => {
                let m = Select::new("Method:", vec![
                    "LoadLibraryW — Simple & Stable (recommended)",
                    "Reflective — DLL self-maps (needs ReflectiveLoader export)",
                    "ManualMap — Injector maps everything (most stealth)",
                ]).prompt()?;
                method = match m {
                    "LoadLibraryW — Simple & Stable (recommended)" => InjectMethod::LoadLibrary,
                    "Reflective — DLL self-maps (needs ReflectiveLoader export)" => InjectMethod::Reflective,
                    "ManualMap — Injector maps everything (most stealth)" => InjectMethod::ManualMap,
                    _ => unreachable!(),
                };
                println!("✅ Method: {}", method);
            }
            "📁 Add DLL(s)" => {
                let files = rfd::FileDialog::new().add_filter("DLL", &["dll"]).pick_files();
                if let Some(paths) = files {
                    let mut added = 0;
                    for p in paths {
                        if !dlls.contains(&p) { dlls.push(p); added += 1; }
                    }
                    println!("✅ Added {} DLL(s)", added);
                }
            }
            "🗑  Remove DLL" => {
                if dlls.is_empty() { println!("❌ Empty."); continue; }
                let items: Vec<String> = dlls.iter().map(|p| p.display().to_string()).collect();
                let sel = Select::new("Remove:", items).prompt()?;
                if let Some(i) = dlls.iter().position(|p| p.display().to_string() == sel) {
                    let r = dlls.remove(i);
                    println!("✅ Removed {}", r.display());
                }
            }
            "🧹 Clear DLL List" => {
                dlls.clear();
                println!("✅ Cleared.");
            }
            "🚀 Execute Injection" => {
                if pid.is_none() || dlls.is_empty() {
                    println!("❌ Select process and DLL first.");
                    continue;
                }
                let target = pid.unwrap();

                match is_process_64bit(target) {
                    Ok(is_64) => println!("Target is {}-bit", if is_64 { "64" } else { "32" }),
                    Err(e) => { println!("❌ {}", e); continue; }
                }

                println!("\nReady to inject {} DLL(s) via [{}]", dlls.len(), method);
                if !Confirm::new("Execute?").with_default(true).prompt()? { continue; }

                // ---------- 新增：注入覆盖层 core.dll（若存在） ----------
                let core_path = std::env::current_exe()?.parent().unwrap().join("fdi").join("core.dll");
                if core_path.exists() {
                    println!("🎨 Injecting overlay core...");
                    if let Err(e) = inject(target, &core_path, InjectMethod::LoadLibrary) {
                        println!("⚠️ Overlay injection failed: {}", e);
                    }
                }
                // -------------------------------------------------------

                let mut failed = Vec::new();
                let total = dlls.len();

                for (idx, dll) in dlls.iter().enumerate() {
                    print_progress(idx, total, &format!("Injecting {}...", dll.file_name().unwrap_or_default().to_string_lossy()));
                    match inject(target, dll, method) {
                        Ok(res) => {
                            print_progress(idx + 1, total, "Done");
                            println!("\n   ✅ {} @ 0x{:X}", res.method, res.base_address);
                        }
                        Err(e) => {
                            print_progress(idx + 1, total, "Failed");
                            println!("\n   ❌ {}", e);
                            failed.push((dll.clone(), e));
                        }
                    }
                }

                if failed.is_empty() {
                    println!("\n✅ All injected successfully.");
                } else {
                    println!("\n⚠️ {} failed:", failed.len());
                    for (d, e) in failed { println!("   - {}: {}", d.display(), e); }
                }
                println!("\nPress Enter...");
                let mut buf = String::new();
                std::io::stdin().read_line(&mut buf)?;
            }
            "👋 Exit" => {
                println!("👋 Goodbye!");
                break;
            }
            _ => {}
        }
    }
    logger::info("Exited");
    Ok(())
}