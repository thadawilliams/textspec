use crate::models::MotherboardInfo;

pub fn collect() -> Option<MotherboardInfo> {
    collect_platform()
}

#[cfg(target_os = "windows")]
fn collect_platform() -> Option<MotherboardInfo> {
    // WMI queries:
    //   Win32_BaseBoard  → Manufacturer, Product (model)
    //   Win32_BIOS       → Manufacturer, SMBIOSBIOSVersion, ReleaseDate
    //
    // Full WMI wiring will go here once the wmi crate integration is added.
    // Stubbed for now — returns None so the binary still compiles and runs.
    //
    // Example WMI query pattern (pseudo):
    //   let wmi_con = WMIConnection::new(...)?;
    //   let results: Vec<Win32BaseBoard> = wmi_con.query()?;
    None
}

#[cfg(target_os = "linux")]
fn collect_platform() -> Option<MotherboardInfo> {
    // Requires dmidecode (usually needs sudo, but many distros allow type 0/1/2)
    use std::process::Command;

    let board = run_dmidecode("baseboard")?;
    let bios = run_dmidecode("bios");

    let manufacturer = extract_dmi_field(&board, "Manufacturer")?;
    let model = extract_dmi_field(&board, "Product Name")?;

    let (bios_vendor, bios_version, bios_date) = if let Some(b) = bios {
        (
            extract_dmi_field(&b, "Vendor"),
            extract_dmi_field(&b, "Version"),
            extract_dmi_field(&b, "Release Date"),
        )
    } else {
        (None, None, None)
    };

    Some(MotherboardInfo {
        manufacturer,
        model,
        bios_vendor,
        bios_version,
        bios_date,
    })
}

#[cfg(target_os = "macos")]
fn collect_platform() -> Option<MotherboardInfo> {
    // system_profiler SPHardwareDataType
    use std::process::Command;
    let output = Command::new("system_profiler")
        .arg("SPHardwareDataType")
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;

    let model = extract_sp_field(&text, "Model Identifier")?;

    Some(MotherboardInfo {
        manufacturer: "Apple".to_string(),
        model,
        bios_vendor: None,
        bios_version: None,
        bios_date: None,
    })
}

// ── Linux helpers ─────────────────────────────────────────────────────────────
#[cfg(target_os = "linux")]
fn run_dmidecode(dmi_type: &str) -> Option<String> {
    use std::process::Command;
    let output = Command::new("dmidecode")
        .args(["-t", dmi_type])
        .output()
        .ok()?;
    if output.status.success() {
        String::from_utf8(output.stdout).ok()
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn extract_dmi_field(text: &str, field: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(field) {
            if let Some(val) = trimmed.splitn(2, ':').nth(1) {
                let clean = val.trim().to_string();
                if !clean.is_empty() && clean != "Not Specified" {
                    return Some(clean);
                }
            }
        }
    }
    None
}

// ── macOS helpers ─────────────────────────────────────────────────────────────
#[cfg(target_os = "macos")]
fn extract_sp_field(text: &str, field: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(field) {
            return trimmed.splitn(2, ':').nth(1).map(|s| s.trim().to_string());
        }
    }
    None
}
