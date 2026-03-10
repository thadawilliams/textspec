// ── gpu.rs ────────────────────────────────────────────────────────────────────
use sysinfo::System;
use crate::models::GpuInfo;

pub fn collect(_sys: &System) -> Vec<GpuInfo> {
    collect_platform()
}

#[cfg(target_os = "windows")]
fn collect_platform() -> Vec<GpuInfo> {
    use wmi::{COMLibrary, WMIConnection};
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    #[serde(rename = "Win32_VideoController")]
    #[serde(rename_all = "PascalCase")]
    struct Win32VideoController {
        name: Option<String>,
        driver_version: Option<String>,
    }

    let com = match COMLibrary::new() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let wmi = match WMIConnection::new(com) {
        Ok(w) => w,
        Err(_) => return vec![],
    };
    let results: Vec<Win32VideoController> = match wmi.query() {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    // Get accurate VRAM from DXGI via PowerShell — bypasses the WMI 32-bit cap
    let dxgi_vram = get_dxgi_vram();

    results.into_iter().filter_map(|r| {
        let name = r.name?.trim().to_string();
        if name.is_empty() { return None; }

        let is_integrated = is_integrated_gpu(&name);

        // Match DXGI VRAM by adapter name substring
        let vram_mb = dxgi_vram.iter()
            .find(|(adapter_name, _)| {
                let a = adapter_name.to_lowercase();
                let n = name.to_lowercase();
                a.contains(&n) || n.contains(&a)
            })
            .map(|(_, mb)| *mb);

        Some(GpuInfo {
            name,
            vram_mb,
            driver_version: r.driver_version,
            is_integrated,
            shared_memory_mb: None,
        })
    }).collect()
}

/// Query DXGI for accurate VRAM — returns Vec of (adapter_name, vram_mb)
#[cfg(target_os = "windows")]
fn get_dxgi_vram() -> Vec<(String, u64)> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory, IDXGIFactory};

    let mut results = Vec::new();

    let factory: IDXGIFactory = match unsafe { CreateDXGIFactory() } {
        Ok(f) => f,
        Err(_) => return results,
    };

    let mut i = 0u32;
    loop {
        let adapter = match unsafe { factory.EnumAdapters(i) } {
            Ok(a) => a,
            Err(_) => break,
        };
        i += 1;

        let desc = match unsafe { adapter.GetDesc() } {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Description is a null-terminated UTF-16 array
        let name_utf16: Vec<u16> = desc.Description.iter()
            .map(|&c| c as u16)
            .take_while(|&c| c != 0)
            .collect();
        let name = String::from_utf16_lossy(&name_utf16);

        let vram_bytes = desc.DedicatedVideoMemory;
        let vram_mb = vram_bytes as u64 / (1024 * 1024);

        if vram_mb > 0 {
            results.push((name, vram_mb));
        }
    }
    results
}

#[cfg(target_os = "windows")]
fn is_integrated_gpu(name: &str) -> bool {
    let n = name.to_lowercase();
    (n.contains("intel") && (n.contains("uhd") || n.contains("hd graphics") || n.contains("iris") || n.contains("arc") && n.contains("graphics")))
    || (n.contains("amd") && n.contains("radeon") && !n.contains("rx") && !n.contains("pro") && !n.contains("vega") && n.contains("graphics"))
    || n.contains("microsoft basic display")
    || n.contains("vmware") || n.contains("virtualbox")
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
