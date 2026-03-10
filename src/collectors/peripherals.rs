use crate::models::{PeripheralDevice, PeripheralKind};

pub fn collect() -> Vec<PeripheralDevice> {
    collect_platform()
}

#[cfg(target_os = "windows")]
fn collect_platform() -> Vec<PeripheralDevice> {
    use wmi::{COMLibrary, WMIConnection};
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    #[serde(rename = "Win32_PnPEntity")]
    #[serde(rename_all = "PascalCase")]
    struct Win32PnPEntity {
        name: Option<String>,
        manufacturer: Option<String>,
        #[serde(rename = "PNPClass")]
        pnp_class: Option<String>,
        device_id: Option<String>,
    }

    let com = match COMLibrary::new() {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let wmi = match WMIConnection::new(com) {
        Ok(w) => w,
        Err(_) => return vec![],
    };
    let results: Vec<Win32PnPEntity> = match wmi.query() {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let mut peripherals: Vec<PeripheralDevice> = results
        .into_iter()
        .filter_map(|r| {
            let name = r.name?.trim().to_string();
            let pnp_class = r.pnp_class.unwrap_or_default().to_lowercase();
            let manufacturer = r.manufacturer
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let device_id = r.device_id.unwrap_or_default().to_lowercase();

            // Only consider relevant PnP classes
            // AudioEndpoint is excluded — it creates virtual "Microsoft Speakers/Microphone"
            // entries for every physical audio device, which is pure noise
            let is_relevant = matches!(pnp_class.as_str(),
                "hidclass" | "media" | "keyboard" | "mouse"
            );
            if !is_relevant { return None; }

            // Filter out internal/virtual/noise entries
            if is_noise(&name, &manufacturer, &device_id) { return None; }

            let kind = classify_peripheral(&name, &pnp_class);

            // The device_id root (minus the last \-separated segment) is stable
            // across multiple profile registrations for the same physical device.
            // e.g. BTHENUM\{guid}\...\BLUETOOTHDEVICE_XXX_STEREO
            //  and BTHENUM\{guid}\...\BLUETOOTHDEVICE_XXX_HFP
            // share everything up to the last segment.
            let device_id_root = {
                let parts: Vec<&str> = device_id.rsplitn(2, '\\').collect();
                if parts.len() == 2 { parts[1].to_string() } else { device_id.clone() }
            };

            Some(PeripheralDevice {
                name,
                manufacturer,
                kind,
                device_id_root,
            })
        })
        .collect();

    // Deduplicate — same physical device can register multiple times for different
    // audio profiles (A2DP stereo, HFP hands-free, input/output endpoints).
    // Primary key: device_id_root (same hardware path = same device).
    // Fallback: name suffix stripping for cases where IDs differ.
    peripherals.sort_by(|a, b| a.device_id_root.cmp(&b.device_id_root).then(a.name.cmp(&b.name)));
    peripherals.dedup_by(|a, b| {
        // Same device ID root = same physical device
        if a.device_id_root == b.device_id_root && !a.device_id_root.is_empty() {
            return true;
        }
        // Fallback: same base name after stripping profile suffixes
        strip_audio_suffix(&a.name) == strip_audio_suffix(&b.name)
    });

    // Sort: Audio first, then Keyboard, Mouse, Other
    peripherals.sort_by_key(|p| match p.kind {
        PeripheralKind::Audio    => 0,
        PeripheralKind::Keyboard => 1,
        PeripheralKind::Mouse    => 2,
        PeripheralKind::Other    => 3,
    });

    peripherals
}

#[cfg(target_os = "windows")]
fn strip_audio_suffix(name: &str) -> &str {
    // Windows registers audio devices multiple times for different profiles/roles.
    // Bluetooth: A2DP stereo + HFP hands-free
    // USB mics: input + output endpoints
    // Strip known suffixes so dedup treats them as the same device.
    const SUFFIXES: &[&str] = &[
        " Hands-Free AG Audio",
        " Hands-Free",
        " Stereo Microphone",
        " Stereo",
        " Microphone",
        " Speakers",
        " Headphones",
        " Headset",
        " LE Audio",
        " Avrcp Transport",
        " Audio",
    ];
    for suffix in SUFFIXES {
        if let Some(base) = name.strip_suffix(suffix) {
            if !base.is_empty() {
                return base;
            }
        }
    }
    name
}

#[cfg(target_os = "windows")]
fn is_noise(name: &str, manufacturer: &Option<String>, device_id: &str) -> bool {
    let n = name.to_lowercase();
    let mfr = manufacturer.as_deref().unwrap_or("").to_lowercase();

    // Internal GPU/APU audio — not a peripheral
    if (mfr.contains("amd") || mfr.contains("advanced micro devices") || mfr.contains("nvidia"))
        && (n.contains("high definition audio") || n.contains("streaming audio") || n.contains("hdmi audio"))
    {
        return true;
    }

    // Generic noise names
    let noise_names = [
        "hid-compliant", "usb input device", "usb composite device",
        "usb root hub", "usb hub", "generic usb hub",
        "virtual", "composite",
        "system speaker", "pc speaker",
        "hid keyboard device", "hid-keyboard", "hid mouse",
    ];
    if noise_names.iter().any(|&n_pat| n.contains(n_pat)) {
        return true;
    }

    // Names that start with "Microsoft " are Windows virtual/software audio endpoints
    if n.starts_with("microsoft ") {
        return true;
    }

    // Internal bus devices (not USB peripherals)
    if device_id.starts_with("acpi\\") || device_id.starts_with("pci\\") {
        // Allow AMD/Realtek audio that's genuinely USB (device_id will have USB\\)
        if !device_id.contains("usb\\") && !device_id.contains("usbvid") {
            return true;
        }
    }

    false
}

#[cfg(target_os = "windows")]
fn classify_peripheral(name: &str, pnp_class: &str) -> PeripheralKind {
    let n = name.to_lowercase();
    if pnp_class == "media" || pnp_class == "audioendpoint"
        || n.contains("audio") || n.contains("dac") || n.contains("microphone")
        || n.contains("headset") || n.contains("headphone") || n.contains("speaker")
        || n.contains("sound")
    {
        return PeripheralKind::Audio;
    }
    if pnp_class == "keyboard" || n.contains("keyboard") {
        return PeripheralKind::Keyboard;
    }
    if pnp_class == "mouse" || n.contains("mouse") {
        return PeripheralKind::Mouse;
    }
    PeripheralKind::Other
}

#[cfg(target_os = "linux")]
fn collect_platform() -> Vec<PeripheralDevice> {
    // lsusb gives us connected USB devices
    use std::process::Command;
    let output = match Command::new("lsusb").output() {
        Ok(o) => o,
        Err(_) => return vec![],
    };
    let text = String::from_utf8(output.stdout).unwrap_or_default();
    text.lines().filter_map(|line| {
        // Format: "Bus 001 Device 002: ID 046d:c52b Logitech, Inc. Unifying Receiver"
        let desc = line.splitn(4, ':').nth(2).unwrap_or("").trim();
        if desc.is_empty() || desc.to_lowercase().contains("hub") { return None; }
        Some(PeripheralDevice {
            name: desc.to_string(),
            ..Default::default()
        })
    }).collect()
}

#[cfg(target_os = "macos")]
fn collect_platform() -> Vec<PeripheralDevice> {
    vec![] // system_profiler SPUSBDataType — future implementation
}
