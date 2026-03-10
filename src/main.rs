mod models;
mod output;
mod collectors;

use clap::Parser;
use sysinfo::System;
use models::SystemSnapshot;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "textspec")]
#[command(version)]
#[command(about = "Clean hardware spec snapshot for posting anywhere", long_about = None)]
struct Cli {
    /// Copy output to clipboard automatically
    #[arg(short, long)]
    copy: bool,

    /// Save output to a file
    #[arg(short, long, value_name = "FILE")]
    output: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    let start = Instant::now();
    eprint!("Collecting system information...");

    let mut sys = System::new_all();
    sys.refresh_all();

    let snapshot = SystemSnapshot {
        cpu:         collectors::cpu::collect(&sys),
        motherboard: collectors::motherboard::collect(),
        ram:         collectors::ram::collect(),
        gpus:        collectors::gpu::collect(&sys),
        displays:    collectors::display::collect(),
        storage:     collectors::storage::collect(&sys),
        peripherals: collectors::peripherals::collect(),
        os:          collectors::os::collect(&sys),
    };

    let elapsed = start.elapsed();
    eprintln!(" done ({:.1}s)", elapsed.as_secs_f32());

    let text = output::format_snapshot(&snapshot);

    println!("{}", text);

    if let Some(path) = cli.output {
        match std::fs::write(&path, &text) {
            Ok(_) => eprintln!("Saved to {}", path),
            Err(e) => eprintln!("Failed to write file: {}", e),
        }
    }

    if cli.copy {
        copy_to_clipboard(&text);
    }

    // If launched by double-clicking from Explorer (no parent console),
    // pause so the user can read the output before the window closes.
    #[cfg(target_os = "windows")]
    if launched_from_explorer() {
        eprintln!("\nPress Enter to close...");
        let mut input = String::new();
        let _ = std::io::stdin().read_line(&mut input);
    }
}

/// Returns true if the process was launched by double-clicking in Explorer
/// rather than from an existing terminal session.
/// Detects this by checking if the parent process is explorer.exe.
#[cfg(target_os = "windows")]
fn launched_from_explorer() -> bool {
    use std::process::Command;
    // Get our own PID
    let our_pid = std::process::id();
    // Ask WMIC for our parent PID
    let output = Command::new("wmic")
        .args(["process", "where", &format!("ProcessId={}", our_pid),
               "get", "ParentProcessId", "/value"])
        .output();
    let parent_pid_str = match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return false,
    };
    let parent_pid: u32 = parent_pid_str
        .lines()
        .find(|l| l.starts_with("ParentProcessId="))
        .and_then(|l| l.split('=').nth(1))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);

    if parent_pid == 0 { return false; }

    // Check if that parent is explorer.exe
    let output = Command::new("wmic")
        .args(["process", "where", &format!("ProcessId={}", parent_pid),
               "get", "Name", "/value"])
        .output();
    let parent_name = match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return false,
    };
    parent_name.to_lowercase().contains("explorer.exe")
}

fn copy_to_clipboard(text: &str) {
    #[cfg(target_os = "windows")]
    {
        use std::process::{Command, Stdio};
        use std::io::Write;
        if let Ok(mut child) = Command::new("clip").stdin(Stdio::piped()).spawn() {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            eprintln!("Copied to clipboard.");
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::process::{Command, Stdio};
        use std::io::Write;
        for cmd in &["xclip -selection clipboard", "xsel --clipboard --input"] {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if let Ok(mut child) = Command::new(parts[0]).args(&parts[1..]).stdin(Stdio::piped()).spawn() {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    eprintln!("Copied to clipboard.");
                    return;
                }
            }
        }
        eprintln!("Could not copy to clipboard (install xclip or xsel).");
    }
    #[cfg(target_os = "macos")]
    {
        use std::process::{Command, Stdio};
        use std::io::Write;
        if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            eprintln!("Copied to clipboard.");
        }
    }
}
