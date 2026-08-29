// src/main.rs
use clap::Parser;
use inquire::{Select, Confirm, Text};
use std::path::PathBuf;
use std::process;
use std::io::Write;

use WeaveRift::injector::{inject, is_process_64bit, InjectMethod};
use WeaveRift::process_finder;
use WeaveRift::logger;
use WeaveRift::ipc;

// ─── CLI 参数 ─────────────────────────────────────────────────────────────
#[derive(Parser)]
#[command(name = "WeaveRift")]
#[command(author = "FastNow Studio")]
#[command(version = "1.0.3")]
#[command(about = "Weave through the rift.", long_about = None)]
struct Cli {
    #[arg(short, long, value_parser = ["classic", "weave"])]
    mode: Option<String>,

    #[arg(short, long)]
    pid: Option<u32>,

    #[arg(long)]
    dll: Option<PathBuf>,

    #[arg(long)]
    agent: Option<PathBuf>,
}

// ─── 全局应用状态 ──────────────────────────────────────────────────────────
struct AppState {
    pid: Option<u32>,
    method: InjectMethod,
    dlls: Vec<PathBuf>,
    agent_path: Option<PathBuf>,
    mode: String, // "classic" 或 "weave"
}

impl AppState {
    fn new() -> Self {
        Self {
            pid: None,
            method: InjectMethod::LoadLibrary,
            dlls: Vec::new(),
            agent_path: None,
            mode: "classic".to_string(),
        }
    }
}

// ─── 辅助函数 ──────────────────────────────────────────────────────────────
fn print_banner() {
    println!(
        r#"
╔═══════════════════════════════════════════════════════════════════╗
║                                                                   ║
║   ██╗    ██╗███████╗ █████╗ ██╗   ██╗███████╗██████╗ ██╗███████╗████████╗
║   ██║    ██║██╔════╝██╔══██╗██║   ██║██╔════╝██╔══██╗██║██╔════╝╚══██╔══╝
║   ██║ █╗ ██║█████╗  ███████║██║   ██║█████╗  ██████╔╝██║█████╗     ██║   
║   ██║███╗██║██╔══╝  ██╔══██║╚██╗ ██╔╝██╔══╝  ██╔══██╗██║██╔══╝     ██║   
║   ╚███╔███╔╝███████╗██║  ██║ ╚████╔╝ ███████╗██║  ██║██║██║        ██║   
║    ╚══╝╚══╝ ╚══════╝╚═╝  ╚═╝  ╚═══╝  ╚══════╝╚═╝  ╚═╝╚═╝╚═╝        ╚═╝   
║                                                                   ║
║            Weave through the rift.                               ║
║            Version 1.0.3                                         ║
╚═══════════════════════════════════════════════════════════════════╝
"#
    );
}

fn print_status(state: &AppState) {
    let mode_label = if state.mode == "weave" { "🧵 Weave" } else { "💉 Classic" };
    let target = state.pid.map_or("Not selected".to_string(), |p| format!("PID {}", p));
    let dll_count = state.dlls.len();
    let agent = state.agent_path.as_ref().map_or("None".to_string(), |p| p.display().to_string());

    println!("\n┌──────────────────────────────────────────────────────┐");
    println!("│  Mode   : {}", mode_label);
    println!("│  Target : {}", target);
    if state.mode == "classic" {
        println!("│  Method : {}", state.method);
        println!("│  DLLs   : {} file(s)", dll_count);
        for (i, d) in state.dlls.iter().enumerate() {
            println!("│           {}. {}", i+1, d.display());
        }
    } else {
        println!("│  Agent  : {}", agent);
    }
    println!("└──────────────────────────────────────────────────────┘\n");
}

fn print_progress(current: usize, total: usize, msg: &str) {
    let pct = (current * 100) / total;
    let filled = (pct * 20) / 100;
    let bar: String = std::iter::repeat('█').take(filled)
        .chain(std::iter::repeat('░').take(20 - filled)).collect();
    print!("\r   [{}] {:>3}% - {}", bar, pct, msg);
    std::io::stdout().flush().ok();
}

// ─── 核心功能函数 ──────────────────────────────────────────────────────────

/// 扫描并选择目标进程
fn select_process() -> Option<u32> {
    println!("🔄 Scanning for Minecraft processes...");
    let procs = process_finder::find_minecraft_processes();
    if procs.is_empty() {
        println!("❌ No Minecraft process found.");
        return None;
    }

    if procs.len() == 1 {
        let p = &procs[0];
        println!("✅ Auto-selected PID {} ({})", p.pid, p.name);
        return Some(p.pid);
    }

    let items: Vec<String> = procs.iter()
        .map(|p| format!("PID {:6}  {}  ({})", p.pid, p.name, p.title))
        .collect();
    let sel = Select::new("Select target process:", items.clone()).prompt().ok()?;
    let idx = items.iter().position(|s| s == &sel)?;
    Some(procs[idx].pid)
}

/// 切换注入方式（仅 Classic 模式）
fn select_inject_method() -> InjectMethod {
    let opts = vec![
        "LoadLibraryW — Simple & Stable (recommended)",
        "Reflective — DLL self-maps (needs ReflectiveLoader export)",
        "ManualMap — Injector maps everything (most stealth)",
    ];
    let sel = Select::new("Select injection method:", opts.clone())
        .prompt()
        .unwrap_or_else(|_| opts[0]);
    match sel {
        "LoadLibraryW — Simple & Stable (recommended)" => InjectMethod::LoadLibrary,
        "Reflective — DLL self-maps (needs ReflectiveLoader export)" => InjectMethod::Reflective,
        "ManualMap — Injector maps everything (most stealth)" => InjectMethod::ManualMap,
        _ => InjectMethod::LoadLibrary,
    }
}

/// 添加 DLL（Classic 模式）
fn add_dlls(state: &mut AppState) {
    let files = rfd::FileDialog::new()
        .add_filter("DLL", &["dll"])
        .pick_files();
    if let Some(paths) = files {
        let mut count = 0;
        for p in paths {
            if !state.dlls.contains(&p) {
                state.dlls.push(p);
                count += 1;
            }
        }
        println!("✅ Added {} DLL(s)", count);
    }
}

/// 移除 DLL
fn remove_dll(state: &mut AppState) {
    if state.dlls.is_empty() {
        println!("❌ No DLLs to remove.");
        return;
    }
    let items: Vec<String> = state.dlls.iter().map(|p| p.display().to_string()).collect();
    let sel = Select::new("Select DLL to remove:", items.clone()).prompt().ok();
    if let Some(sel_str) = sel {
        if let Some(idx) = items.iter().position(|s| s == &sel_str) {
            let removed = state.dlls.remove(idx);
            println!("✅ Removed {}", removed.display());
        }
    }
}

/// 切换模式（Classic <-> Weave）
fn toggle_mode(state: &mut AppState) {
    state.mode = if state.mode == "classic" { "weave".to_string() } else { "classic".to_string() };
    println!("✅ Switched to {} mode", state.mode);
}

/// 选择 Agent JAR（Weave 模式）
fn select_agent(state: &mut AppState) {
    let file = rfd::FileDialog::new()
        .add_filter("JAR", &["jar"])
        .pick_file();
    if let Some(path) = file {
        state.agent_path = Some(path);
        println!("✅ Agent selected: {}", state.agent_path.as_ref().unwrap().display());
    } else {
        println!("❌ No file selected.");
    }
}

// ─── 执行注入 ──────────────────────────────────────────────────────────────

fn execute_injections(pid: u32, dlls: &[PathBuf], method: InjectMethod) -> Result<(), String> {
    let total = dlls.len();
    let mut failed = Vec::new();

    for (idx, dll) in dlls.iter().enumerate() {
        print_progress(idx, total, &format!("Injecting {}...", dll.file_name().unwrap_or_default().to_string_lossy()));
        match inject(pid, dll, method) {
            Ok(res) => {
                print_progress(idx + 1, total, "Done");
                println!("\n   ✅ {} @ 0x{:X}", res.method, res.base_address);
            }
            Err(e) => {
                print_progress(idx + 1, total, "Failed");
                println!("\n   ❌ {}", e);
                failed.push((dll.display().to_string(), e));
            }
        }
    }

    if failed.is_empty() {
        println!("\n✅ All injections succeeded.");
    } else {
        println!("\n⚠️  {} injection(s) failed:", failed.len());
        for (name, err) in failed {
            println!("   - {}: {}", name, err);
        }
    }
    Ok(())
}

fn run_classic_mode(state: &AppState) -> Result<(), String> {
    let pid = state.pid.ok_or("No target process selected.")?;
    let dlls = &state.dlls;
    if dlls.is_empty() {
        return Err("No DLLs selected.".to_string());
    }

    // 检查位数
    for dll in dlls {
        if let Err(e) = WeaveRift::injector::pe_util::check_dll_architecture(dll, pid) {
            return Err(format!("DLL '{}' architecture mismatch: {}", dll.display(), e));
        }
    }

    // 注入动画 core.dll（统一放在 WeaveRift 子目录）
    let exe_dir = std::env::current_exe().map_err(|e| e.to_string())?.parent().unwrap().to_path_buf();
    let core_path = exe_dir.join("WeaveRift").join("core.dll");
    if core_path.exists() {
        println!("🎨 Injecting overlay core (WeaveRift animation)...");
        if let Err(e) = inject(pid, &core_path, InjectMethod::LoadLibrary) {
            println!("⚠️ Overlay injection failed: {}", e);
        } else {
            println!("✅ Overlay core injected.");
        }
    } else {
        println!("ℹ️ core.dll not found, skipping animation.");
    }

    execute_injections(pid, dlls, state.method)?;
    Ok(())
}

fn run_weave_mode(state: &AppState) -> Result<(), String> {
    let pid = state.pid.ok_or("No target process selected.")?;
    let agent_path = state.agent_path.as_ref().ok_or("No Agent JAR selected.")?;

    // 检查目标进程是否 64 位
    if !is_process_64bit(pid).map_err(|e| e.to_string())? {
        return Err("Weave mode requires a 64-bit Java process.".to_string());
    }

    // 注入动画 core.dll
    let exe_dir = std::env::current_exe().map_err(|e| e.to_string())?.parent().unwrap().to_path_buf();
    let core_path = exe_dir.join("WeaveRift").join("core.dll");
    if core_path.exists() {
        println!("🎨 Injecting overlay core (WeaveRift animation)...");
        if let Err(e) = inject(pid, &core_path, InjectMethod::LoadLibrary) {
            println!("⚠️ Overlay injection failed: {}", e);
        } else {
            println!("✅ Overlay core injected.");
        }
    } else {
        println!("ℹ️ core.dll not found, skipping animation.");
    }

    // 通过共享内存传递 Agent 路径
    ipc::write_agent_path(agent_path).map_err(|e| format!("IPC write failed: {}", e))?;

    // 注入 loader DLL（从 WeaveRift 子目录）
    let loader_dll = exe_dir.join("WeaveRift").join("jar_loader.dll");
    if !loader_dll.exists() {
        return Err("jar_loader.dll not found in release/WeaveRift folder.".to_string());
    }

    println!("🧵 Loading jar_loader.dll...");
    let res = inject(pid, &loader_dll, InjectMethod::LoadLibrary)
        .map_err(|e| format!("Failed to inject jar_loader.dll: {}", e))?;
    println!("✅ jar_loader.dll injected @ 0x{:X}", res.base_address);

    println!("⏳ Waiting for Agent to attach...");
    std::thread::sleep(std::time::Duration::from_secs(2));
    println!("✅ Weave injection completed (assuming success).");

    Ok(())
}

// ─── 主菜单 ──────────────────────────────────────────────────────────────

fn interactive_loop() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = AppState::new();

    loop {
        print_banner();
        print_status(&state);

        let mut options = vec![
            "🎯 Select Target Process",
            "🔄 Switch Mode (Classic/Weave)",
        ];

        if state.mode == "classic" {
            options.extend_from_slice(&[
                "💉 Select Injection Method",
                "📁 Add DLL(s)",
                "🗑  Remove DLL",
                "🧹 Clear DLL List",
            ]);
        } else {
            options.push("📦 Select Agent JAR");
        }

        options.push("🚀 Execute Injection");
        options.push("👋 Exit");

        let choice = Select::new("Select action:", options).prompt()?;

        match choice {
            "🎯 Select Target Process" => {
                if let Some(pid) = select_process() {
                    state.pid = Some(pid);
                }
            }
            "🔄 Switch Mode (Classic/Weave)" => {
                toggle_mode(&mut state);
                // 清空无关数据
                if state.mode == "classic" {
                    state.agent_path = None;
                } else {
                    state.dlls.clear();
                }
            }
            "💉 Select Injection Method" => {
                if state.mode == "classic" {
                    state.method = select_inject_method();
                }
            }
            "📁 Add DLL(s)" => {
                if state.mode == "classic" {
                    add_dlls(&mut state);
                }
            }
            "🗑  Remove DLL" => {
                if state.mode == "classic" {
                    remove_dll(&mut state);
                }
            }
            "🧹 Clear DLL List" => {
                if state.mode == "classic" {
                    state.dlls.clear();
                    println!("✅ DLL list cleared.");
                }
            }
            "📦 Select Agent JAR" => {
                if state.mode == "weave" {
                    select_agent(&mut state);
                }
            }
            "🚀 Execute Injection" => {
                if state.pid.is_none() {
                    println!("❌ No target process selected.");
                    continue;
                }
                // 确认
                let confirm_msg = match state.mode.as_str() {
                    "classic" => format!("Inject {} DLL(s) via [{}]?", state.dlls.len(), state.method),
                    "weave" => "Load Java Agent via Weave Mode?".to_string(),
                    _ => unreachable!(),
                };
                if !Confirm::new(&confirm_msg).with_default(true).prompt()? {
                    continue;
                }

                // 执行
                let result = if state.mode == "classic" {
                    run_classic_mode(&state)
                } else {
                    run_weave_mode(&state)
                };

                match result {
                    Ok(_) => println!("✅ Injection completed."),
                    Err(e) => println!("❌ Injection failed: {}", e),
                }

                println!("\nPress Enter to continue...");
                let mut buf = String::new();
                std::io::stdin().read_line(&mut buf)?;
            }
            "👋 Exit" => {
                println!("👋 Goodbye!");
                break;
            }
            _ => unreachable!(),
        }

        // 短暂停顿让用户看清反馈
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    Ok(())
}

// ─── 主入口 ──────────────────────────────────────────────────────────────

fn main() -> Result<(), Box<dyn std::error::Error>> {
    logger::init_logger().ok();
    logger::info(&format!("WeaveRift v{} started", env!("CARGO_PKG_VERSION")));

    let cli = Cli::parse();

    // 如果提供了任何 CLI 参数，进入 CLI 模式
    if cli.mode.is_some() || cli.pid.is_some() || cli.dll.is_some() || cli.agent.is_some() {
        let mode = cli.mode.as_deref().unwrap_or("classic");
        let pid = cli.pid.ok_or("--pid is required in CLI mode")?;

        match mode {
            "classic" => {
                let dlls = cli.dll.map(|p| vec![p]).unwrap_or_default();
                if dlls.is_empty() {
                    eprintln!("❌ --dll is required for classic mode");
                    process::exit(1);
                }
                let mut state = AppState::new();
                state.pid = Some(pid);
                state.dlls = dlls;
                state.method = InjectMethod::LoadLibrary; // 默认

                if let Err(e) = run_classic_mode(&state) {
                    eprintln!("❌ {}", e);
                    process::exit(1);
                }
            }
            "weave" => {
                let agent = cli.agent.ok_or("--agent is required for weave mode")?;
                let mut state = AppState::new();
                state.pid = Some(pid);
                state.agent_path = Some(agent);
                state.mode = "weave".to_string();

                if let Err(e) = run_weave_mode(&state) {
                    eprintln!("❌ {}", e);
                    process::exit(1);
                }
            }
            _ => unreachable!(),
        }
        return Ok(());
    }

    // 否则进入交互式菜单
    interactive_loop()?;
    Ok(())
}