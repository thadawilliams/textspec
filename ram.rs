use crate::models::RamStick;

pub fn collect() -> Vec<RamStick> {
    collect_platform()
}

#[cfg(target_os = "windows")]
fn collect_platform() -> Vec<RamStick> {
    // WMI: SELECT * FROM Win32_PhysicalMemory
    // Fields: DeviceLocator, Manufacturer, PartNumber, Capacity,
    //         Speed, MemoryType, SMBIOSMemoryType
    //
    // Timings (CL, tRCD, tRP, tRAS) are NOT available via WMI.
    // They require reading SPD data from the memory controller,
    // which needs either a kernel driver or a tool like CPU-Z.
    // We will expose them as None unless a future enhancement
    // adds a privileged SPD reader.
    //
    // Stubbed pending WMI wiring.
    vec![]
}

#[cfg(target_os = "linux")]
fn collect_platform() -> Vec<RamStick> {
    use std::process::Command;

    let output = match Command::new("dmidecode").args(["-t", "memory"]).output() {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let text = match String::from_utf8(output.stdout) {
        Ok(t) => t,
        Err(_) => return vec![],
    };

    parse_dmidecode_memory(&text)
}

#[cfg(target_os = "macos")]
fn collect_platform() -> Vec<RamStick> {
    use std::process::Command;

    let output = match Command::new("system_profiler")
        .arg("SPMemoryDataType")
        .output()
    {
        Ok(o) => o,
        Err(_) => return vec![],
    };

    let text = match String::from_utf8(output.stdout) {
        Ok(t) => t,
        Err(_) => return vec![],
    };

    parse_system_profiler_memory(&text)
}

// ── Linux: parse dmidecode -t memory ─────────────────────────────────────────
#[cfg(target_os = "linux")]
fn parse_dmidecode_memory(text: &str) -> Vec<RamStick> {
    let mut sticks = vec![];
    let mut current: Option<RamStick> = None;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed == "Memory Device" {
            if let Some(stick) = current.take() {
                if stick.capacity_mb > 0 {
                    sticks.push(stick);
                }
            }
            current = Some(RamStick::default());
            continue;
        }

        if let Some(ref mut stick) = current {
            if let Some(val) = field_val(trimmed, "Locator") {
                stick.slot = Some(val);
            } else if let Some(val) = field_val(trimmed, "Manufacturer") {
                if val != "Not Specified" {
                    stick.manufacturer = Some(val);
                }
            } else if let Some(val) = field_val(trimmed, "Part Number") {
                if val != "Not Specified" {
                    stick.part_number = Some(val.trim().to_string());
                }
            } else if let Some(val) = field_val(trimmed, "Size") {
                stick.capacity_mb = parse_size_to_mb(&val).unwrap_or(0);
            } else if let Some(val) = field_val(trimmed, "Speed") {
                stick.speed_mhz = val.split_whitespace().next().and_then(|v| v.parse().ok());
            } else if let Some(val) = field_val(trimmed, "Type") {
                stick.memory_type = Some(val);
            }
        }
    }

    if let Some(stick) = current {
        if stick.capacity_mb > 0 {
            sticks.push(stick);
        }
    }

    sticks
}

#[cfg(target_os = "linux")]
fn field_val(line: &str, field: &str) -> Option<String> {
    if line.starts_with(field) {
        line.splitn(2, ':').nth(1).map(|s| s.trim().to_string())
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn parse_size_to_mb(val: &str) -> Option<u64> {
    let parts: Vec<&str> = val.split_whitespace().collect();
    if parts.len() < 2 { return None; }
    let num: u64 = parts[0].parse().ok()?;
    match parts[1].to_uppercase().as_str() {
        "MB" => Some(num),
        "GB" => Some(num * 1024),
        _ => None,
    }
}

// ── macOS: parse system_profiler SPMemoryDataType ────────────────────────────
#[cfg(target_os = "macos")]
fn parse_system_profiler_memory(text: &str) -> Vec<RamStick> {
    // system_profiler output is indentation-based; simple line parse
    let mut sticks = vec![];
    let mut current: Option<RamStick> = None;

    for line in text.lines() {
        let trimmed = line.trim();

        // Slot headers look like "DIMM0/J1:" or "Bank 0/DIMM0:"
        if trimmed.ends_with(':') && (trimmed.contains("DIMM") || trimmed.contains("Bank")) {
            if let Some(stick) = current.take() {
                if stick.capacity_mb > 0 { sticks.push(stick); }
            }
            current = Some(RamStick { slot: Some(trimmed.trim_end_matches(':').to_string()), ..Default::default() });
            continue;
        }

        if let Some(ref mut stick) = current {
            if let Some(val) = sp_field(trimmed, "Size") {
                stick.capacity_mb = parse_size_to_mb_mac(&val).unwrap_or(0);
            } else if let Some(val) = sp_field(trimmed, "Speed") {
                stick.speed_mhz = val.split_whitespace().next().and_then(|v| v.parse().ok());
            } else if let Some(val) = sp_field(trimmed, "Type") {
                stick.memory_type = Some(val);
            } else if let Some(val) = sp_field(trimmed, "Manufacturer") {
                stick.manufacturer = Some(val);
            } else if let Some(val) = sp_field(trimmed, "Part Number") {
                stick.part_number = Some(val);
            }
        }
    }

    if let Some(stick) = current {
        if stick.capacity_mb > 0 { sticks.push(stick); }
    }
    sticks
}

#[cfg(target_os = "macos")]
fn sp_field(line: &str, field: &str) -> Option<String> {
    if line.starts_with(field) {
        line.splitn(2, ':').nth(1).map(|s| s.trim().to_string())
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn parse_size_to_mb_mac(val: &str) -> Option<u64> {
    let parts: Vec<&str> = val.split_whitespace().collect();
    if parts.len() < 2 { return None; }
    let num: u64 = parts[0].parse().ok()?;
    match parts[1].to_uppercase().as_str() {
        "MB" => Some(num),
        "GB" => Some(num * 1024),
        _ => None,
    }
}
