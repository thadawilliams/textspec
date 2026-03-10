use crate::models::RamStick;

pub fn collect() -> Vec<RamStick> {
    collect_platform()
}

#[cfg(target_os = "windows")]
fn collect_platform() -> Vec<RamStick> {
    use wmi::{COMLibrary, WMIConnection};
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    #[serde(rename = "Win32_PhysicalMemory")]
    #[serde(rename_all = "PascalCase")]
    struct Win32PhysicalMemory {
        device_locator: Option<String>,
        bank_label: Option<String>,
        manufacturer: Option<String>,
        part_number: Option<String>,
        capacity: Option<u64>,
        speed: Option<u32>,
        // SMBIOSMemoryType is the correct field for DDR generation
        // MemoryType is often 0 on modern systems
        s_m_b_i_o_s_memory_type: Option<u16>,
        configured_clock_speed: Option<u32>,
    }

    let com = match COMLibrary::new() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let wmi = match WMIConnection::new(com) {
        Ok(w) => w,
        Err(_) => return vec![],
    };
    let results: Vec<Win32PhysicalMemory> = match wmi.query() {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let mut sticks: Vec<RamStick> = results.into_iter().map(|r| {
        let capacity_mb = r.capacity.unwrap_or(0) / 1_048_576;
        let speed_mhz = r.configured_clock_speed.or(r.speed);
        let memory_type = smbios_memory_type(r.s_m_b_i_o_s_memory_type.unwrap_or(0));

        let slot = match (r.bank_label.as_deref(), r.device_locator.as_deref()) {
            (Some(b), Some(d)) if b.trim() != d.trim() => Some(format!("{} / {}", b.trim(), d.trim())),
            (_, Some(d)) => Some(d.trim().to_string()),
            (Some(b), _) => Some(b.trim().to_string()),
            _ => None,
        };

        let manufacturer = r.manufacturer
            .map(|s| s.trim().to_string())
            .map(|s| resolve_manufacturer_id(&s))
            .filter(|s| {
                let l = s.to_lowercase();
                !s.is_empty()
                    && l != "unknown"
                    && l != "not specified"
                    && l != "not available"
                    && l != "jedec"
            });

        let part_number = r.part_number
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.to_lowercase() != "unknown");

        RamStick {
            slot,
            manufacturer,
            part_number,
            capacity_mb,
            speed_mhz,
            memory_type,
            cas_latency: None,
            trcd: None,
            trp: None,
            tras: None,
        }
    }).filter(|s| s.capacity_mb > 0).collect();

    // If multiple sticks share the same slot label (firmware limitation),
    // append a counter to distinguish them e.g. "DIMM 1 (1)", "DIMM 1 (2)"
    let mut slot_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for stick in &sticks {
        let key = stick.slot.clone().unwrap_or_default();
        *slot_counts.entry(key).or_insert(0) += 1;
    }
    let mut slot_seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for stick in &mut sticks {
        let key = stick.slot.clone().unwrap_or_default();
        if slot_counts.get(&key).copied().unwrap_or(0) > 1 {
            let count = slot_seen.entry(key.clone()).or_insert(0);
            *count += 1;
            stick.slot = Some(format!("{} ({})", key, count));
        }
    }
    sticks
}

#[cfg(target_os = "windows")]
fn smbios_memory_type(code: u16) -> Option<String> {
    match code {
        15 => Some("DDR".into()),
        18 => Some("DDR2".into()),
        20 => Some("DDR2 FB-DIMM".into()),
        24 => Some("DDR3".into()),
        26 => Some("DDR4".into()),
        27 => Some("LPDDR".into()),
        28 => Some("LPDDR2".into()),
        29 => Some("LPDDR3".into()),
        30 => Some("LPDDR4".into()),
        34 | 35 => Some("DDR5".into()),
        36 => Some("LPDDR5".into()),
        _  => None,
    }
}

#[cfg(target_os = "windows")]
fn resolve_manufacturer_id(raw: &str) -> String {
    // WMI returns JEDEC manufacturer IDs as hex strings on some systems.
    // If it already looks like a real name (contains letters beyond hex),
    // pass it through directly.
    let upper = raw.to_uppercase();
    let trimmed = upper.trim();

    // Detect if it's a hex ID: all chars are 0-9, A-F, spaces
    let is_hex = trimmed.chars().all(|c| c.is_ascii_hexdigit() || c == ' ');
    if !is_hex && trimmed.len() > 4 {
        return raw.trim().to_string();
    }

    match trimmed {
        "80CE" | "CE80" | "CE00" | "00CE" => "Samsung".into(),
        "AD00" | "80AD" | "AD80" | "00AD" => "SK Hynix".into(),
        "2C00" | "802C" | "2C80" | "002C" => "Micron".into(),
        "0198" | "9801"                   => "Kingston".into(),
        "04CD" | "CD04"                   => "G.Skill".into(),
        "0420" | "2004"                   => "Corsair".into(),
        "1B85" | "851B"                   => "ADATA".into(),
        "1A5D" | "5D1A"                   => "TeamGroup".into(),
        "1551" | "5115"                   => "Patriot".into(),
        "04E6" | "E604"                   => "PNY".into(),
        "048F" | "8F04"                   => "Transcend".into(),
        "04B3" | "B304"                   => "Ramaxel".into(),
        "0194" | "9401"                   => "Smart".into(),
        "013F" | "3F01"                   => "Apacer".into(),
        "04F1" | "F104"                   => "Unifosa".into(),
        "02FE" | "FE02"                   => "Elpida".into(),
        "0543" | "4305"                   => "Qimonda".into(),
        other                             => other.to_string(),
    }
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
