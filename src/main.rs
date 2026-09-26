// src/main.rs
use clap::Parser;
use inquire::{Select, Confirm};
use std::path::PathBuf;
use std::process;
use std::time::Duration;

use WeaveRift::injector::{inject, is_process_64bit, InjectMethod};
use WeaveRift::process_finder;
use WeaveRift::logger;
use WeaveRift::ipc;

// ─── CLI 参数 ─────────────────────────────────────────────────────────────
#[derive(Parser)]
#[command(name = "WeaveRift")]
#[command(author = "FastNow Studio")]
#[command(version = "1.0.2")]
#[command(about = "Weave through the rift.", long_about = None)]
struct Cli {
    #[arg(short, long)]
    pid: Option<u32>,

    #[arg(long)]
    agent: Option<PathBuf>,
}

// ─── 全局应用状态 ──────────────────────────────────────────────────────────
struct AppState {
    pid: Option<u32>,
    agent_jar: Option<PathBuf>,
}

impl AppState {
    fn new() -> Self {
        Self {
            pid: None,
            agent_jar: None,
        }
    }
}

// ─── 辅助函数 ──────────────────────────────────────────────────────────────
fn print_banner() {
    println!(
        r#"
╔═════════════════════════════════════════════════════════════════════════════╗
║                                                                             ║
║   ██╗    ██╗███████╗ █████╗ ██╗   ██╗███████╗██████╗ ██╗███████╗████████╗   ║
║   ██║    ██║██╔════╝██╔══██╗██║   ██║██╔════╝██╔══██╗██║██╔════╝╚══██╔══╝   ║
║   ██║ █╗ ██║█████╗  ███████║██║   ██║█████╗  ██████╔╝██║█████╗     ██║      ║
║   ██║███╗██║██╔══╝  ██╔══██║╚██╗ ██╔╝██╔══╝  ██╔══██╗██║██╔══╝     ██║      ║
║   ╚███╔███╔╝███████╗██║  ██║ ╚████╔╝ ███████╗██║  ██║██║██║        ██║      ║
║    ╚══╝╚══╝ ╚══════╝╚═╝  ╚═╝  ╚═══╝  ╚══════╝╚═╝  ╚═╝╚═╝╚═╝        ╚═╝      ║
║                                                                             ║
║                           Weave through the rift.                           ║
║                                Version 1.0.2                                ║
╚═════════════════════════════════════════════════════════════════════════════╝
"#
    );
}

fn print_status(state: &AppState) {
    let target = state.pid.map_or("(not selected)".to_string(), |p| format!("PID {}", p));
    let agent = state.agent_jar.as_ref().map_or("(not selected)".to_string(), |p| {
        p.file_name().unwrap_or_default().to_string_lossy().to_string()
    });

    println!("\n┌──────────────────────────────────────────────────────┐");
    println!("│  Target : {}", target);
    println!("│  Agent  : {}", agent);
    println!("└──────────────────────────────────────────────────────┘\n");
}

// ─── 核心功能 ──────────────────────────────────────────────────────────────

/// 扫描并选择目标进程
fn select_process() -> Option<u32> {
    println!("🔄 Scanning for Minecraft processes...");
    let procs = process_finder::find_minecraft_processes();
    if procs.is_empty() {
        println!("❌ No Minecraft process found. Please start the game first.");
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

/// 选择 Agent JAR
fn select_agent(state: &mut AppState) {
    let file = rfd::FileDialog::new()
        .add_filter("Java Agent", &["jar"])
        .pick_file();
    if let Some(path) = file {
        state.agent_jar = Some(path);
        println!("✅ Agent selected: {}", state.agent_jar.as_ref().unwrap().display());
    } else {
        println!("❌ No file selected.");
    }
}

/// 执行 Weave 注入（已有 PID）
fn run_weave(state: &AppState) -> Result<(), String> {
    let pid = state.pid.ok_or("No target process selected.")?;
    let jar_path = state.agent_jar.as_ref().ok_or("No Agent JAR selected.")?;

    if !is_process_64bit(pid).map_err(|e| e.to_string())? {
        return Err("Weave mode requires a 64-bit Java process.".to_string());
    }

    let exe_dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot find exe directory")?
        .to_path_buf();

    // ─── 写共享内存 ───────────────────────────────
    println!("📝 Writing Agent path to shared memory...");
    ipc::write_jar_path(jar_path, pid)
        .map_err(|e| format!("IPC write failed: {}", e))?;
    println!("✅ Agent path written.");

    // ─── 注入 jar_loader.dll ──────────────────────
    let loader_dll = exe_dir.join("WeaveRift").join("jar_loader.dll");
    if !loader_dll.exists() {
        return Err(format!(
            "jar_loader.dll not found at: {}",
            loader_dll.display()
        ));
    }

    println!("📦 Injecting jar_loader.dll...");
    let res = inject(pid, &loader_dll, InjectMethod::LoadLibrary)
        .map_err(|e| format!("Failed to inject jar_loader.dll: {}", e))?;
    println!("✅ jar_loader.dll injected @ 0x{:X}", res.base_address);

    println!("⏳ Waiting for Agent to load...");
    std::thread::sleep(Duration::from_secs(2));
    println!("✅ Weave injection completed.");

    Ok(())
}

/// ★ 自动等待进程 + 立即注入
///
/// 不要求用户先启动 Minecraft。程序会轮询 javaw.exe，
/// 一发现就立刻注入，赶在 Minecraft 类加载之前把 transformer 装好。
fn auto_wait_and_inject(state: &AppState) -> Result<(), String> {
    let jar_path = state.agent_jar.as_ref().ok_or("No Agent JAR selected.")?;

    // 检查 SRG 文件在不在（Agent 会去读）
    let srg_path = jar_path.parent()
        .ok_or("JAR has no parent dir")?
        .join("obf2srg.srg");
    if !srg_path.exists() {
        return Err(format!("obf2srg.srg not found at: {}", srg_path.display()));
    }

    let exe_dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot find exe directory")?
        .to_path_buf();

    let loader_dll = exe_dir.join("WeaveRift").join("jar_loader.dll");
    if !loader_dll.exists() {
        return Err(format!("jar_loader.dll not found at: {}", loader_dll.display()));
    }

    println!();
    println!("════════════════════════════════════════════════════════");
    println!("  ⏳ Waiting for Minecraft (javaw.exe)...");
    println!("  📌 Please start Minecraft NOW (PCL / HMCL / official)");
    println!("  ⛔ Press Ctrl+C to cancel");
    println!("════════════════════════════════════════════════════════");
    println!();

    // 轮询等待进程出现
    let start = std::time::Instant::now();
    let pid = loop {
        let procs = process_finder::find_minecraft_processes();
        if let Some(p) = procs.first() {
            println!();
            println!("✅ Found Minecraft: PID {} ({})", p.pid, p.name);
            break p.pid;
        }
        if start.elapsed().as_secs() > 300 {
            return Err("Timeout: no Minecraft process found in 5 minutes.".to_string());
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    // ★ 立即注入（不等窗口出现）
    println!("📝 Writing Agent path to shared memory...");
    ipc::write_jar_path(jar_path, pid)
        .map_err(|e| format!("IPC write failed: {}", e))?;
    println!("✅ Agent path written.");

    println!("📦 Injecting jar_loader.dll...");
    let res = inject(pid, &loader_dll, InjectMethod::LoadLibrary)
        .map_err(|e| format!("Failed to inject: {}", e))?;
    println!("✅ jar_loader.dll injected @ 0x{:X}", res.base_address);

    println!();
    println!("════════════════════════════════════════════════════════");
    println!("  ✅ Injection complete!");
    println!("  📌 Watch the Minecraft log for [WeaveRift] messages");
    println!("════════════════════════════════════════════════════════");
    println!();

    Ok(())
}

// ─── 主菜单 ──────────────────────────────────────────────────────────────

fn interactive_loop() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = AppState::new();

    // 启动时自动扫一次
    println!("🔄 Auto-scanning for Minecraft...");
    if let Some(pid) = select_process() {
        state.pid = Some(pid);
    }

    loop {
        print_banner();
        print_status(&state);

        let mut options = Vec::new();

        // 进程
        if state.pid.is_none() {
            options.push("🎯 Select Target Process".to_string());
        } else {
            options.push("🎯 Change Target Process".to_string());
        }

        // Agent
        if state.agent_jar.is_none() {
            options.push("📦 Select Agent JAR".to_string());
        } else {
            options.push("📦 Change Agent JAR".to_string());
        }

        // ★ 两种注入模式
        if state.agent_jar.is_some() {
            options.push("🚀 Inject Now (require game already running)".to_string());
            options.push("⏳ Auto-Wait & Inject (start this BEFORE Minecraft)".to_string());
        }

        options.push("👋 Exit".to_string());

        let choice = Select::new("Select action:", options.clone()).prompt()?;

        match &choice[..] {
            "🎯 Select Target Process" | "🎯 Change Target Process" => {
                if let Some(pid) = select_process() {
                    state.pid = Some(pid);
                }
            }
            "📦 Select Agent JAR" | "📦 Change Agent JAR" => {
                select_agent(&mut state);
            }
            "🚀 Inject Now (require game already running)" => {
                if state.pid.is_none() {
                    println!("❌ No target process selected.");
                    continue;
                }
                let confirm_msg = format!(
                    "Inject Agent '{}' into PID {}?",
                    state.agent_jar.as_ref().unwrap().file_name()
                        .unwrap_or_default().to_string_lossy(),
                    state.pid.unwrap()
                );
                if !Confirm::new(&confirm_msg).with_default(true).prompt()? {
                    continue;
                }

                match run_weave(&state) {
                    Ok(_) => println!("✅ Injection completed."),
                    Err(e) => println!("❌ Injection failed: {}", e),
                }

                println!("\nPress Enter to continue...");
                let mut buf = String::new();
                std::io::stdin().read_line(&mut buf)?;
            }
            "⏳ Auto-Wait & Inject (start this BEFORE Minecraft)" => {
                let confirm_msg = "This will poll for Minecraft. Start Minecraft AFTER confirming.\nProceed?";
                if !Confirm::new(confirm_msg).with_default(true).prompt()? {
                    continue;
                }

                match auto_wait_and_inject(&state) {
                    Ok(_) => println!("✅ Auto-injection completed."),
                    Err(e) => println!("❌ Auto-injection failed: {}", e),
                }

                println!("\nPress Enter to continue...");
                let mut buf = String::new();
                std::io::stdin().read_line(&mut buf)?;
            }
            "👋 Exit" => {
                println!("👋 Goodbye!");
                break;
            }
            _ => {}
        }

        std::thread::sleep(Duration::from_millis(300));
    }
    Ok(())
}

// ─── 主入口 ──────────────────────────────────────────────────────────────

fn main() -> Result<(), Box<dyn std::error::Error>> {
    logger::init_logger().ok();
    logger::info(&format!("WeaveRift v{} started", env!("CARGO_PKG_VERSION")));

    let cli = Cli::parse();

    // CLI 模式
    if cli.pid.is_some() || cli.agent.is_some() {
        let mut state = AppState::new();
        if let Some(pid) = cli.pid {
            state.pid = Some(pid);
        }
        if let Some(agent) = cli.agent {
            state.agent_jar = Some(agent);
        }

        if state.agent_jar.is_some() && state.pid.is_some() {
            if let Err(e) = run_weave(&state) {
                eprintln!("❌ {}", e);
                process::exit(1);
            }
        } else if state.agent_jar.is_some() {
            // 只传了 agent，自动等待
            if let Err(e) = auto_wait_and_inject(&state) {
                eprintln!("❌ {}", e);
                process::exit(1);
            }
        } else {
            eprintln!("❌ --agent is required");
            process::exit(1);
        }
        return Ok(());
    }

    interactive_loop()?;
    Ok(())
}