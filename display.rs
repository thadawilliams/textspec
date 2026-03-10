use crate::models::DisplayInfo;

pub fn collect() -> Vec<DisplayInfo> {
    collect_platform()
}

#[cfg(target_os = "windows")]
fn collect_platform() -> Vec<DisplayInfo> {
    // WMI: SELECT * FROM Win32_DesktopMonitor  (limited - often misses refresh rate)
    // Better: EnumDisplayDevices + EnumDisplaySettings via windows crate
    // TODO: windows crate wiring
    vec![]
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
