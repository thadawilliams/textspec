use crate::models::DisplayInfo;

pub fn collect() -> Vec<DisplayInfo> {
    collect_platform()
}

#[cfg(target_os = "windows")]
fn collect_platform() -> Vec<DisplayInfo> {
    use std::process::Command;

    // Strategy:
    // 1. QueryDisplayConfig gives us paths in display order with TargetID and accurate refresh rate
    // 2. Each TargetID matches a UID{n} suffix in the DISPLAY registry keys
    // 3. We read the EDID name from that registry key
    // 4. AllScreens gives us resolution and primary flag, indexed by sourceInfo.id
    let script = r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class QDC {
    [DllImport("user32.dll")] public static extern int GetDisplayConfigBufferSizes(uint f, out uint np, out uint nm);
    [DllImport("user32.dll")] public static extern int QueryDisplayConfig(uint f, ref uint np, [Out] PATH[] paths, ref uint nm, [Out] MODE[] modes, IntPtr tid);
    [StructLayout(LayoutKind.Sequential)] public struct PATH {
        public SRC src; public TGT tgt; public uint flags;
    }
    [StructLayout(LayoutKind.Sequential)] public struct SRC {
        public LUID adapter; public uint id; public uint modeIdx; public uint status;
    }
    [StructLayout(LayoutKind.Sequential)] public struct TGT {
        public LUID adapter; public uint id; public uint modeIdx;
        public uint tech; public uint rot; public uint scale;
        public RAT refresh; public uint scanline; public bool avail; public uint status;
    }
    [StructLayout(LayoutKind.Sequential)] public struct RAT { public uint N; public uint D; }
    [StructLayout(LayoutKind.Sequential)] public struct LUID { public uint Lo; public int Hi; }
    [StructLayout(LayoutKind.Sequential)] public struct MODE {
        public uint infoType; public uint id; public LUID adapter;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst=64)] public byte[] data;
    }
}
'@

# Build UID -> EDID name map from registry
$uidNameMap = @{}
$dispKeys = Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Enum\DISPLAY" -ErrorAction SilentlyContinue
foreach ($mfr in $dispKeys) {
    $mfrName = $mfr.PSChildName
    $instances = Get-ChildItem $mfr.PSPath -ErrorAction SilentlyContinue
    foreach ($inst in $instances) {
        if ($inst.PSChildName -match "UID(\d+)") {
            $uid = [int]$matches[1]
            $regPath = "HKLM:\SYSTEM\CurrentControlSet\Enum\DISPLAY\$mfrName\$($inst.PSChildName)\Device Parameters"
            $edid = (Get-ItemProperty $regPath -ErrorAction SilentlyContinue).EDID
            if (-not $edid) { continue }
            for ($i = 54; $i -le 108; $i += 18) {
                if ($edid[$i] -eq 0 -and $edid[$i+1] -eq 0 -and $edid[$i+2] -eq 0 -and $edid[$i+3] -eq 0xFC) {
                    $name = [System.Text.Encoding]::ASCII.GetString($edid[($i+5)..($i+17)]).Trim()
                    $uidNameMap[$uid] = $name
                    break
                }
            }
        }
    }
}

# QueryDisplayConfig for authoritative path order, TargetID, and refresh rate
$np = 0; $nm = 0
[QDC]::GetDisplayConfigBufferSizes(2, [ref]$np, [ref]$nm) | Out-Null
$paths = New-Object QDC+PATH[] $np
$modes = New-Object QDC+MODE[] $nm
[QDC]::QueryDisplayConfig(2, [ref]$np, $paths, [ref]$nm, $modes, [IntPtr]::Zero) | Out-Null

$screens = [System.Windows.Forms.Screen]::AllScreens

foreach ($path in $paths) {
    $srcIdx  = $path.src.id                   # matches AllScreens index
    $tgtId   = [int]$path.tgt.id                   # matches UID in registry (cast to int to match map keys)
    $rN      = $path.tgt.refresh.N
    $rD      = $path.tgt.refresh.D
    $refresh = if ($rD -gt 0) { [math]::Round($rN / $rD) } else { 0 }

    $name = if ($uidNameMap.ContainsKey($tgtId)) { $uidNameMap[$tgtId] } else { "" }

    $screen = $screens | Select-Object -Index $srcIdx
    if (-not $screen) { continue }
    $primary = if ($screen.Primary) { "1" } else { "0" }
    $w = $screen.Bounds.Width
    $h = $screen.Bounds.Height

    Write-Output "$primary|$name|$w|$h|$refresh"
}
"#;

    let output = {
        // Write to a temp .ps1 file — complex scripts with nested here-strings
        // are unreliable when passed via -Command
        use std::fs;
        let tmp = std::env::temp_dir().join("textspec_display.ps1");
        if fs::write(&tmp, script).is_err() { return vec![]; }
        let result = Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                   tmp.to_str().unwrap_or("")])
            .output();
        let _ = fs::remove_file(&tmp);
        match result {
            Ok(o) => o,
            Err(_) => return vec![],
        }
    };

    let text = String::from_utf8(output.stdout).unwrap_or_default();
    let mut displays = vec![];

    for line in text.lines() {
        let parts: Vec<&str> = line.trim().split('|').collect();
        if parts.len() < 5 { continue; }

        let is_primary = parts[0] == "1";
        let name = parts[1].trim().to_string();
        let w: u32 = parts[2].parse().unwrap_or(0);
        let h: u32 = parts[3].parse().unwrap_or(0);
        let refresh: f64 = parts[4].parse().unwrap_or(0.0);

        if w == 0 { continue; }

        displays.push(DisplayInfo {
            name: if name.is_empty() { None } else { Some(name) },
            resolution_w: w,
            resolution_h: h,
            refresh_rate_hz: if refresh > 0.0 { Some(refresh) } else { None },
            is_primary,
        });
    }
    displays
}

#[cfg(target_os = "linux")]
fn collect_platform() -> Vec<DisplayInfo> {
    use std::process::Command;

    // Try xrandr first (X11), then fall back to nothing for Wayland for now
    let output = match Command::new("xrandr").output() {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let text = String::from_utf8(output.stdout).unwrap_or_default();
    parse_xrandr(&text)
}

#[cfg(target_os = "macos")]
fn collect_platform() -> Vec<DisplayInfo> {
    use std::process::Command;
    let output = match Command::new("system_profiler").arg("SPDisplaysDataType").output() {
        Ok(o) => o,
        Err(_) => return vec![],
    };
    let text = String::from_utf8(output.stdout).unwrap_or_default();
    parse_sp_displays(&text)
}

// ── Linux: xrandr parser ──────────────────────────────────────────────────────
#[cfg(target_os = "linux")]
fn parse_xrandr(text: &str) -> Vec<DisplayInfo> {
    let mut displays = vec![];
    let mut first = true;

    for line in text.lines() {
        // Connected output lines: "HDMI-1 connected primary 2560x1440+0+0 ..."
        if line.contains(" connected") {
            let is_primary = line.contains(" primary ");
            let mut info = DisplayInfo {
                is_primary: is_primary && first,
                ..Default::default()
            };

            // Extract resolution from mode section (next non-empty indented line with *)
            // We'll capture the output name as display name for now
            let name = line.split_whitespace().next().unwrap_or("").to_string();
            info.name = Some(name);
            displays.push(info);
            first = false;
        } else if line.starts_with("   ") && line.contains('*') {
            // Current mode line e.g. "   2560x1440     59.95*+"
            if let Some(display) = displays.last_mut() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(res) = parts.first() {
                    let dims: Vec<&str> = res.split('x').collect();
                    if dims.len() == 2 {
                        display.resolution_w = dims[0].parse().unwrap_or(0);
                        display.resolution_h = dims[1].parse().unwrap_or(0);
                    }
                }
                // Refresh rate is the number with * after it
                for part in &parts[1..] {
                    if part.contains('*') {
                        let rate_str = part.trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.');
                        display.refresh_rate_hz = rate_str.parse().ok();
                        break;
                    }
                }
            }
        }
    }
    displays
}

// ── macOS: system_profiler display parser ─────────────────────────────────────
#[cfg(target_os = "macos")]
fn parse_sp_displays(text: &str) -> Vec<DisplayInfo> {
    let mut displays = vec![];
    let mut current: Option<DisplayInfo> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with(':') && line.starts_with("        ") {
            if let Some(d) = current.take() { displays.push(d); }
            current = Some(DisplayInfo {
                name: Some(trimmed.trim_end_matches(':').to_string()),
                is_primary: displays.is_empty(),
                ..Default::default()
            });
        } else if let Some(ref mut d) = current {
            if trimmed.starts_with("Resolution:") {
                if let Some(val) = trimmed.splitn(2, ':').nth(1) {
                    // "2560 x 1440"
                    let nums: Vec<u32> = val.split('x')
                        .filter_map(|s| s.trim().split_whitespace().next()?.parse().ok())
                        .collect();
                    if nums.len() >= 2 {
                        d.resolution_w = nums[0];
                        d.resolution_h = nums[1];
                    }
                }
            } else if trimmed.starts_with("Refresh Rate:") {
                if let Some(val) = trimmed.splitn(2,':').nth(1) {
                    d.refresh_rate_hz = val.trim().split_whitespace().next().and_then(|v| v.parse().ok());
                }
            }
        }
    }
    if let Some(d) = current { displays.push(d); }
    displays
}
