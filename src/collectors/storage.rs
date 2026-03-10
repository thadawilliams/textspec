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
    #[serde(rename = "MSFT_PhysicalDisk")]
    #[serde(rename_all = "PascalCase")]
    struct MsftPhysicalDisk {
        friendly_name: Option<String>,
        size: Option<u64>,
        media_type: Option<u16>,  // 0=Unspecified, 3=HDD, 4=SSD, 5=SCM
        bus_type: Option<u16>,    // 3=ATA, 11=SATA, 17=NVMe, 7=USB, 9=SD, etc.
    }

    let com = match COMLibrary::new() {
        Ok(c) => c,
        Err(_) => return,
    };
    // MSFT_PhysicalDisk lives in a different namespace than Win32
    let wmi = match WMIConnection::with_namespace_path("ROOT\\Microsoft\\Windows\\Storage", com) {
        Ok(w) => w,
        Err(_) => return,
    };
    let results: Vec<MsftPhysicalDisk> = match wmi.query() {
        Ok(r) => r,
        Err(_) => return,
    };

    devices.clear();
    for disk in results {
        let model = disk.friendly_name.unwrap_or_default().trim().to_string();
        if model.is_empty() { continue; }

        let capacity_gb = disk.size.unwrap_or(0) as f64 / 1_073_741_824.0;

        // BusType is the authoritative interface signal
        let bus_type = disk.bus_type.unwrap_or(0);
        let media_type = disk.media_type.unwrap_or(0);

        let device_type = match (media_type, bus_type) {
            (_, 17) => StorageType::NvmeSsd,          // BusType NVMe
            (4, _)  => StorageType::SataSsd,           // MediaType SSD, non-NVMe bus
            (3, _)  => StorageType::Hdd,               // MediaType HDD
            (_, 7)  => StorageType::Unknown,           // USB — skip classifying
            _       => StorageType::Unknown,
        };

        // Convert bus type number to a clean label — only show if meaningful
        let interface = bus_type_label(bus_type);

        devices.push(StorageDevice {
            model,
            capacity_gb,
            device_type,
            interface,
        });
    }
}

#[cfg(target_os = "windows")]
fn bus_type_label(bus_type: u16) -> Option<String> {
    match bus_type {
        3  => Some("ATA".into()),
        11 => Some("SATA".into()),
        17 => Some("NVMe".into()),
        7  => Some("USB".into()),
        9  => Some("SD".into()),
        15 => Some("MMC".into()),
        _  => None,   // Don't show confusing labels like SCSI/IDE
    }
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
