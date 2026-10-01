// src/main.rs
mod ipc;
use clap::Parser;
use inquire::{Select, Confirm};
use std::path::PathBuf;
use std::process;
use std::time::Duration;

use WeaveRift::injector::{inject, is_process_64bit, InjectMethod};
use WeaveRift::process_finder;
use WeaveRift::logger;

// ─── CLI 参数 ─────────────────────────────────────

#[derive(Parser)]
#[command(name = "WeaveRift")]
#[command(author = "FastNow Studio")]
#[command(version = "2.0.0")]
#[command(about = "Weave through the rift.", long_about = None)]
struct Cli {
    #[arg(short, long)]
    pid: Option<u32>,

    #[arg(long)]
    srg: Option<PathBuf>,
}

// ─── 全局状态 ─────────────────────────────────────

struct AppState {
    pid: Option<u32>,
    srg_path: Option<PathBuf>,
}

impl AppState {
    fn new() -> Self {
        Self {
            pid: None,
            srg_path: None,
        }
    }
}

// ─── 辅助函数 ─────────────────────────────────────

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
║                                Version 2.0.0                                ║
╚═════════════════════════════════════════════════════════════════════════════╝
"#
    );
}

fn print_status(state: &AppState) {
    let target = state
        .pid
        .map_or("(not selected)".to_string(), |p| format!("PID {}", p));
    let srg = state
        .srg_path
        .as_ref()
        .map_or("(auto-detect)".to_string(), |p| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

    println!("\n┌──────────────────────────────────────────────────────┐");
    println!("│  Target : {}", target);
    println!("│  SRG    : {}", srg);
    println!("└──────────────────────────────────────────────────────┘\n");
}

// ─── 进程选择 ─────────────────────────────────────

fn select_process() -> Option<u32> {
    println!("🔄 扫描 Minecraft 进程（只显示有窗口的）...");
    let procs = process_finder::find_minecraft_windows();
    if procs.is_empty() {
        println!("❌ 未找到有窗口的 Minecraft 进程。");
        println!("   请确认游戏已进入主菜单或已进入世界。");
        return None;
    }

    if procs.len() == 1 {
        let p = &procs[0];
        println!("✅ 自动选中 PID {} ({})", p.pid, p.title);
        return Some(p.pid);
    }

    let items: Vec<String> = procs
        .iter()
        .map(|p| format!("PID {:6}  {}  [{}]", p.pid, p.name, p.title))
        .collect();
    let sel = Select::new("选择目标进程:", items.clone()).prompt().ok()?;
    let idx = items.iter().position(|s| s == &sel)?;
    Some(procs[idx].pid)
}

// ─── SRG 选择 ─────────────────────────────────────

fn select_srg(state: &mut AppState) {
    let file = rfd::FileDialog::new()
        .add_filter("SRG", &["srg"])
        .set_title("选择 obf2srg.srg")
        .pick_file();
    if let Some(path) = file {
        state.srg_path = Some(path.clone());
        println!("✅ SRG: {}", path.display());
    } else {
        println!("❌ 未选择文件");
    }
}

// ─── 注入 ─────────────────────────────────────────

fn run_inject(state: &AppState) -> Result<(), String> {
    let pid = state.pid.ok_or("未选择目标进程")?;

    if !is_process_64bit(pid).map_err(|e| e.to_string())? {
        return Err("目标进程不是 64 位".into());
    }

    let exe_dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("找不到 exe 目录")?
        .to_path_buf();

    let wr_dir = exe_dir.join("WeaveRift");

    let loader_dll = wr_dir.join("jar_loader.dll");
    if !loader_dll.exists() {
        return Err(format!("找不到 jar_loader.dll: {}", loader_dll.display()));
    }

    let agent_jar = wr_dir.join("weaverift-agent.jar");
    if !agent_jar.exists() {
        return Err(format!("找不到 weaverift-agent.jar: {}", agent_jar.display()));
    }

    println!("📝 写共享内存...");
    ipc::write_jar_path(&agent_jar, pid)
        .map_err(|e| format!("IPC 写失败: {}", e))?;

    // 注入 jar_loader.dll
    println!("📦 注入 jar_loader.dll...");
    let res = inject(pid, &loader_dll, InjectMethod::LoadLibrary)
        .map_err(|e| format!("注入失败: {}", e))?;
    println!("✅ jar_loader.dll @ 0x{:X}", res.base_address);

    println!("⏳ 等待 agent 加载（jar_loader 自己会加载）...");
    std::thread::sleep(Duration::from_secs(5));

    println!("✅ 注入完成");
    println!("   调试台见 %TEMP%\\WeaveRift\\debug-url.txt");

    Ok(())
}


// ─── 交互菜单 ─────────────────────────────────────

fn interactive_loop() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = AppState::new();

    loop {
        print_banner();
        print_status(&state);

        let mut options: Vec<String> = Vec::new();

        if state.pid.is_none() {
            options.push("🎯 选择目标进程".to_string());
        } else {
            options.push("🎯 重新选择进程".to_string());
        }

        options.push("📄 选择 SRG 文件（可选）".to_string());

        if state.pid.is_some() {
            options.push("🚀 注入".to_string());
        }

        options.push("👋 退出".to_string());

        let choice = Select::new("选择操作:", options.clone()).prompt()?;

        match choice.as_str() {
            "🎯 选择目标进程" | "🎯 重新选择进程" => {
                if let Some(pid) = select_process() {
                    state.pid = Some(pid);
                }
            }
            "📄 选择 SRG 文件（可选）" => {
                select_srg(&mut state);
            }
            "🚀 注入" => {
                let confirm = format!(
                    "注入到 PID {}？\n(确保 PCL 已加 -Djdk.attach.allowAttachSelf=true)",
                    state.pid.unwrap()
                );
                if !Confirm::new(&confirm).with_default(true).prompt()? {
                    continue;
                }

                match run_inject(&state) {
                    Ok(_) => println!("\n✅ 完成"),
                    Err(e) => println!("\n❌ 失败: {}", e),
                }

                println!("\n按回车继续...");
                let mut buf = String::new();
                std::io::stdin().read_line(&mut buf)?;
            }
            "👋 退出" => {
                println!("👋 再见");
                break;
            }
            _ => {}
        }

        std::thread::sleep(Duration::from_millis(200));
    }

    Ok(())
}

// ─── 入口 ─────────────────────────────────────────

fn main() -> Result<(), Box<dyn std::error::Error>> {
    logger::init_logger().ok();
    logger::info(&format!("WeaveRift v{} 启动", env!("CARGO_PKG_VERSION")));

    let cli = Cli::parse();

    // CLI 模式
    if cli.pid.is_some() {
        let mut state = AppState::new();
        state.pid = cli.pid;
        state.srg_path = cli.srg;

        if let Err(e) = run_inject(&state) {
            eprintln!("❌ {}", e);
            process::exit(1);
        }
        return Ok(());
    }

    // 交互模式
    interactive_loop()?;
    Ok(())
}