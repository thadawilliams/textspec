use sysinfo::System;
use crate::models::{StorageDevice, StorageType};

pub fn collect(sys: &System) -> Vec<StorageDevice> {
    let mut devices: Vec<StorageDevice> = sys.disks().iter().map(|disk| {
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
    // WMI: SELECT * FROM Win32_DiskDrive
    // Fields: Model, Size, InterfaceType, MediaType
    // TODO: WMI wiring
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
