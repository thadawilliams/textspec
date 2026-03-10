mod models;
mod output;
mod collectors;

use clap::Parser;
use sysinfo::System;
use models::SystemSnapshot;

#[derive(Parser)]
#[command(name = "specsnap")]
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

    println!("Collecting system information...");

    let mut sys = System::new_all();
    sys.refresh_all();

    let snapshot = SystemSnapshot {
        cpu:         collectors::cpu::collect(&sys),
        motherboard: collectors::motherboard::collect(),
        ram:         collectors::ram::collect(),
        gpus:        collectors::gpu::collect(&sys),
        displays:    collectors::display::collect(),
        storage:     collectors::storage::collect(&sys),
        os:          collectors::os::collect(&sys),
    };

    let text = output::format_snapshot(&snapshot);

    println!("\n{}", text);

    if let Some(path) = cli.output {
        match std::fs::write(&path, &text) {
            Ok(_) => println!("Saved to {}", path),
            Err(e) => eprintln!("Failed to write file: {}", e),
        }
    }

    if cli.copy {
        copy_to_clipboard(&text);
    }
}

fn copy_to_clipboard(text: &str) {
    // Cross-platform clipboard via OS commands
    // A proper solution would use the `arboard` crate
    #[cfg(target_os = "windows")]
    {
        use std::process::{Command, Stdio};
        use std::io::Write;
        if let Ok(mut child) = Command::new("clip").stdin(Stdio::piped()).spawn() {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            println!("Copied to clipboard.");
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::process::{Command, Stdio};
        use std::io::Write;
        // Try xclip then xsel
        for cmd in &["xclip -selection clipboard", "xsel --clipboard --input"] {
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if let Ok(mut child) = Command::new(parts[0]).args(&parts[1..]).stdin(Stdio::piped()).spawn() {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    println!("Copied to clipboard.");
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
            println!("Copied to clipboard.");
        }
    }
}
