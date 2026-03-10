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
    use std::process::Command;

    let script = r#"
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

[ComImport, Guid("aec22fb8-76f3-4639-9be0-28eb43a67a2e"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IDXGIObject { void SetPrivateData(); void SetPrivateDataInterface(); void GetPrivateData(); void GetParent(); }

[ComImport, Guid("2411e7e1-12ac-4ccf-bd14-9798e8534dc0"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IDXGIAdapter { void EnumOutputs(); void CheckInterfaceSupport(); 
    [PreserveSig] int GetDesc([Out] out DXGI_ADAPTER_DESC desc); }

[ComImport, Guid("7b7166ec-21c7-44ae-b21a-c9ae321ae369"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IDXGIFactory { 
    void MakeWindowAssociation(); void GetWindowAssociation(); void CreateSwapChain(); void CreateSoftwareAdapter();
    [PreserveSig] int EnumAdapters(uint index, out IDXGIAdapter adapter); }

[StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
public struct DXGI_ADAPTER_DESC {
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string Description;
    public uint VendorId, DeviceId, SubSysId, Revision;
    public IntPtr DedicatedVideoMemory, DedicatedSystemMemory, SharedSystemMemory;
    public long AdapterLuid;
}

public class DXGI {
    [DllImport("dxgi.dll")] public static extern int CreateDXGIFactory(ref Guid riid, out IDXGIFactory ppFactory);
}
"@

$iid = [Guid]"7b7166ec-21c7-44ae-b21a-c9ae321ae369"
$factory = $null
[DXGI]::CreateDXGIFactory([ref]$iid, [ref]$factory) | Out-Null

$i = 0
while ($true) {
    $adapter = $null
    $hr = $factory.EnumAdapters($i, [ref]$adapter)
    if ($hr -ne 0) { break }
    $desc = New-Object DXGI_ADAPTER_DESC
    $adapter.GetDesc([ref]$desc) | Out-Null
    $vramMB = [long]$desc.DedicatedVideoMemory / 1MB
    Write-Output "$($desc.Description)|$vramMB"
    $i++
}
"#;

    let output = match Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
    {
        Ok(o) => o,
        Err(_) => return vec![],
    };

    let text = String::from_utf8(output.stdout).unwrap_or_default();
    text.lines()
        .filter_map(|line| {
            let mut parts = line.trim().splitn(2, '|');
            let name = parts.next()?.trim().to_string();
            let mb: u64 = parts.next()?.trim().parse().ok()?;
            if mb > 0 { Some((name, mb)) } else { None }
        })
        .collect()
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
