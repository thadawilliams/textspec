use sysinfo::{System, Disks};
use crate::models::{StorageDevice, StorageType};

pub fn collect(_sys: &System) -> Vec<StorageDevice> {
    let disks = Disks::new_with_refreshed_list();
    let mut devices: Vec<StorageDevice> = disks.iter().map(|disk| {
        let name = disk.name().to_string_lossy().to_string();
        let capacity_gb = disk.total_space() as f64 / 1_073_741_824.0;
        let model = clean_model_name(&name);

        StorageDevice {
            model,
            capacity_gb,
            device_type: StorageType::Unknown,
            interface: None,
        }
    }).collect();

    // Deduplicate — sysinfo lists partitions, not physical drives
    devices.dedup_by(|a, b| a.model == b.model);

    // Enrich with platform-specific data (type, interface)
    enrich_platform(&mut devices);
    devices
}

#[cfg(target_os = "windows")]
fn enrich_platform(devices: &mut Vec<StorageDevice>) {
    use wmi::{COMLibrary, WMIConnection};
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    #[serde(rename = "Win32_DiskDrive")]
    #[serde(rename_all = "PascalCase")]
    struct Win32DiskDrive {
        model: Option<String>,
        size: Option<u64>,          // bytes
        interface_type: Option<String>, // "IDE", "SCSI", "USB", "NVMe" etc.
        media_type: Option<String>,
    }

    let com = match COMLibrary::new() {
        Ok(c) => c,
        Err(_) => return,
    };
    let wmi = match WMIConnection::new(com) {
        Ok(w) => w,
        Err(_) => return,
    };
    let results: Vec<Win32DiskDrive> = match wmi.query() {
        Ok(r) => r,
        Err(_) => return,
    };

    // Replace sysinfo's partition-based list with actual physical drives
    devices.clear();
    for disk in results {
        let model = disk.model.unwrap_or_default().trim().to_string();
        if model.is_empty() { continue; }

        let capacity_gb = disk.size.unwrap_or(0) as f64 / 1_073_741_824.0;
        let iface = disk.interface_type.as_deref().unwrap_or("").to_uppercase();
        let media = disk.media_type.as_deref().unwrap_or("").to_lowercase();

        let device_type = classify_drive(&model, &iface, &media);

        devices.push(StorageDevice {
            model,
            capacity_gb,
            device_type,
            interface: Some(iface),
        });
    }
}

#[cfg(target_os = "windows")]
fn classify_drive(model: &str, iface: &str, media: &str) -> StorageType {
    let m = model.to_lowercase();
    let i = iface.to_uppercase();
    // NVMe: explicit interface, or model name contains nvme/pcie indicators
    if i == "NVME" || m.contains("nvme") || m.contains("ssdpeknw") || m.contains("ssdpeknu") {
        return StorageType::NvmeSsd;
    }
    // Common NVMe model patterns from known brands
    // Samsung 9xx/8xx/7xx Pro/Evo NVMe, WD Black/Blue SN series, Sabrent Rocket, etc.
    if m.contains("mz-v") || m.contains(" sn") || m.contains("rocket") 
       || m.contains("shpp") || m.contains("firecuda") {
        return StorageType::NvmeSsd;
    }
    // Rotational = HDD
    if media.contains("removable") || m.contains("hdwg") || m.contains("hd") && !m.contains("ssd") {
        return StorageType::Hdd;
    }
    // Everything else fixed is SATA SSD
    if media.contains("fixed") {
        return StorageType::SataSsd;
    }
    StorageType::Unknown
}

#[cfg(target_os = "linux")]
fn enrich_platform(devices: &mut Vec<StorageDevice>) {
    use std::process::Command;
    // lsblk -d -o NAME,MODEL,ROTA,TRAN (ROTA=1 means HDD, TRAN=nvme/sata/usb)
    let output = match Command::new("lsblk")
        .args(["-d", "-o", "NAME,MODEL,ROTA,TRAN", "--noheadings"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return,
    };
    let text = String::from_utf8(output.stdout).unwrap_or_default();

    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 { continue; }
        let model = parts[1].to_string();
        let rota = parts[2];
        let tran = parts[3];

        let device_type = match (rota, tran) {
            (_, "nvme") => StorageType::NvmeSsd,
            ("0", _) => StorageType::SataSsd,
            ("1", _) => StorageType::Hdd,
            _ => StorageType::Unknown,
        };

        // Try to match against our collected devices and update type
        for device in devices.iter_mut() {
            if device.model.contains(&model) || model.contains(&device.model) {
                device.device_type = device_type;
                device.interface = Some(tran.to_uppercase());
                break;
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn enrich_platform(devices: &mut Vec<StorageDevice>) {
    use std::process::Command;
    let output = match Command::new("system_profiler").arg("SPStorageDataType").output() {
        Ok(o) => o,
        Err(_) => return,
    };
    let text = String::from_utf8(output.stdout).unwrap_or_default();

    // Look for "Medium Type:" and "Protocol:" fields
    for device in devices.iter_mut() {
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Medium Type:") {
                let val = trimmed.splitn(2,':').nth(1).unwrap_or("").trim();
                device.device_type = match val {
                    "Solid State Drive" => StorageType::SataSsd,
                    _ => StorageType::Hdd,
                };
            } else if trimmed.starts_with("Protocol:") {
                let val = trimmed.splitn(2,':').nth(1).unwrap_or("").trim().to_string();
                if val.contains("NVMe") {
                    device.device_type = StorageType::NvmeSsd;
                }
                device.interface = Some(val);
            }
        }
    }
}

fn clean_model_name(raw: &str) -> String {
    // Strip leading /dev/ or \\.\PhysicalDriveN
    raw.trim_start_matches("/dev/")
       .trim_start_matches(r"\\.\")
       .to_string()
}
