use crate::models::DisplayInfo;

pub fn collect() -> Vec<DisplayInfo> {
    collect_platform()
}

#[cfg(target_os = "windows")]
fn collect_platform() -> Vec<DisplayInfo> {
    use std::process::Command;

    // Single PowerShell script that gets everything:
    // resolution + refresh via EnumDisplaySettings, friendly name via WMI
    let script = r#"
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class Display {
    [DllImport("user32.dll")] public static extern bool EnumDisplayDevices(string lpDevice, uint iDevNum, ref DISPLAY_DEVICE lpDisplayDevice, uint dwFlags);
    [DllImport("user32.dll")] public static extern bool EnumDisplaySettings(string deviceName, int modeNum, ref DEVMODE devMode);
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Ansi)]
    public struct DISPLAY_DEVICE {
        public int cb;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string DeviceName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string DeviceString;
        public int StateFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string DeviceID;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=128)] public string DeviceKey;
    }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Ansi)]
    public struct DEVMODE {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string dmDeviceName;
        public short dmSpecVersion, dmDriverVersion, dmSize, dmDriverExtra;
        public int dmFields, dmPositionX, dmPositionY, dmDisplayOrientation, dmDisplayFixedOutput;
        public short dmColor, dmDuplex, dmYResolution, dmTTOption, dmCollate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string dmFormName;
        public short dmLogPixels; public int dmBitsPerPel, dmPelsWidth, dmPelsHeight, dmDisplayFlags, dmDisplayFrequency;
    }
}
"@

$i = 0
while ($true) {
    $dev = New-Object Display+DISPLAY_DEVICE
    $dev.cb = [System.Runtime.InteropServices.Marshal]::SizeOf($dev)
    if (-not [Display]::EnumDisplayDevices($null, $i, [ref]$dev, 0)) { break }
    $i++
    # StateFlags: 1=attached, 4=primary. Skip non-attached
    if (($dev.StateFlags -band 1) -eq 0) { continue }
    $isPrimary = if (($dev.StateFlags -band 4) -ne 0) { "1" } else { "0" }

    $mode = New-Object Display+DEVMODE
    $mode.dmSize = [System.Runtime.InteropServices.Marshal]::SizeOf($mode)
    [Display]::EnumDisplaySettings($dev.DeviceName, -1, [ref]$mode) | Out-Null

    # Get friendly monitor name from the monitor sub-device
    $mon = New-Object Display+DISPLAY_DEVICE
    $mon.cb = [System.Runtime.InteropServices.Marshal]::SizeOf($mon)
    $friendlyName = $dev.DeviceName
    if ([Display]::EnumDisplayDevices($dev.DeviceName, 0, [ref]$mon, 0)) {
        if ($mon.DeviceString -and $mon.DeviceString -ne "Generic Monitor") {
            $friendlyName = $mon.DeviceString
        }
    }

    Write-Output "$isPrimary|$friendlyName|$($mode.dmPelsWidth)|$($mode.dmPelsHeight)|$($mode.dmDisplayFrequency)"
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
    let mut displays = vec![];

    for line in text.lines() {
        let parts: Vec<&str> = line.trim().split('|').collect();
        if parts.len() < 5 { continue; }

        let is_primary = parts[0] == "1";
        let name = parts[1].trim().to_string();
        let w: u32 = parts[2].parse().unwrap_or(0);
        let h: u32 = parts[3].parse().unwrap_or(0);
        let refresh: f64 = parts[4].parse().unwrap_or(0.0);

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
