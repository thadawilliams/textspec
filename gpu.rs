// ── gpu.rs ────────────────────────────────────────────────────────────────────
use sysinfo::System;
use crate::models::GpuInfo;

pub fn collect(_sys: &System) -> Vec<GpuInfo> {
    collect_platform()
}

#[cfg(target_os = "windows")]
fn collect_platform() -> Vec<GpuInfo> {
    // WMI: SELECT * FROM Win32_VideoController
    // Fields: Name, AdapterRAM, DriverVersion, VideoProcessor
    // Integrated detection: AdapterRAM == 0 or name contains "Intel"/"AMD Radeon Graphics"
    vec![] // TODO: WMI wiring
}

#[cfg(target_os = "linux")]
fn collect_platform() -> Vec<GpuInfo> {
    use std::process::Command;
    let output = match Command::new("lspci").args(["-mm"]).output() {
        Ok(o) => o,
        Err(_) => return vec![],
    };
    let text = String::from_utf8(output.stdout).unwrap_or_default();
    let mut gpus = vec![];

    for line in text.lines() {
        if line.contains("VGA") || line.contains("Display") || line.contains("3D") {
            let name = parse_lspci_name(line);
            let is_integrated = name.contains("Intel") && !name.contains("Arc");
            gpus.push(GpuInfo {
                name,
                is_integrated,
                ..Default::default()
            });
        }
    }
    gpus
}

#[cfg(target_os = "macos")]
fn collect_platform() -> Vec<GpuInfo> {
    use std::process::Command;
    let output = match Command::new("system_profiler").arg("SPDisplaysDataType").output() {
        Ok(o) => o,
        Err(_) => return vec![],
    };
    let text = String::from_utf8(output.stdout).unwrap_or_default();
    let mut gpus = vec![];
    let mut current_name: Option<String> = None;
    let mut current_vram: Option<u64> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with(':') && line.starts_with("      ") && !trimmed.contains(':') {
            if let Some(name) = current_name.take() {
                gpus.push(GpuInfo { name, vram_mb: current_vram.take(), ..Default::default() });
            }
            current_name = Some(trimmed.trim_end_matches(':').to_string());
        } else if trimmed.starts_with("Chipset Model:") {
            current_name = trimmed.splitn(2,':').nth(1).map(|s| s.trim().to_string());
        } else if trimmed.starts_with("VRAM") {
            current_vram = parse_vram(trimmed);
        }
    }
    if let Some(name) = current_name {
        gpus.push(GpuInfo { name, vram_mb: current_vram, ..Default::default() });
    }
    gpus
}

#[cfg(target_os = "linux")]
fn parse_lspci_name(line: &str) -> String {
    // lspci -mm format: slot "class" "vendor" "device" ...
    let parts: Vec<&str> = line.splitn(5, '"').collect();
    if parts.len() >= 5 {
        format!("{} {}", parts[3], parts[4].trim_matches('"').trim())
    } else {
        line.to_string()
    }
}

#[cfg(target_os = "macos")]
fn parse_vram(line: &str) -> Option<u64> {
    let val = line.splitn(2, ':').nth(1)?.trim().to_string();
    let parts: Vec<&str> = val.split_whitespace().collect();
    if parts.len() < 2 { return None; }
    let num: u64 = parts[0].parse().ok()?;
    match parts[1].to_uppercase().as_str() {
        "MB" => Some(num),
        "GB" => Some(num * 1024),
        _ => None,
    }
}
