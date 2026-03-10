use sysinfo::System;
use crate::models::CpuInfo;

pub fn collect(sys: &System) -> Option<CpuInfo> {
    let cpus = sys.cpus();
    if cpus.is_empty() {
        return None;
    }

    let first = &cpus[0];
    let brand = first.brand().trim().to_string();
    if brand.is_empty() {
        return None;
    }

    let threads = cpus.len() as u32;
    let cores = get_physical_cores().unwrap_or(threads / 2);
    let base_clock_mhz = first.frequency() as f64;
    let boost_clock_mhz = get_boost_clock()
        .filter(|&boost| boost > base_clock_mhz + 100.0); // only show if meaningfully higher

    Some(CpuInfo {
        brand,
        cores,
        threads,
        base_clock_mhz,
        boost_clock_mhz,
        cache_l2_kb: get_l2_cache_kb(),
        cache_l3_kb: get_l3_cache_kb(),
    })
}

// ── Windows ───────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn get_physical_cores() -> Option<u32> {
    use wmi::{COMLibrary, WMIConnection};
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(rename = "Win32_Processor")]
    #[serde(rename_all = "PascalCase")]
    struct Win32Processor { number_of_cores: Option<u32> }
    let com = COMLibrary::new().ok()?;
    let wmi = WMIConnection::new(com).ok()?;
    let results: Vec<Win32Processor> = wmi.query().ok()?;
    results.into_iter().next()?.number_of_cores
}

#[cfg(target_os = "windows")]
fn get_boost_clock() -> Option<f64> {
    use wmi::{COMLibrary, WMIConnection};
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(rename = "Win32_Processor")]
    #[serde(rename_all = "PascalCase")]
    struct Win32Processor { max_clock_speed: Option<u32> }
    let com = COMLibrary::new().ok()?;
    let wmi = WMIConnection::new(com).ok()?;
    let results: Vec<Win32Processor> = wmi.query().ok()?;
    results.into_iter().next()?.max_clock_speed.map(|s| s as f64)
}

#[cfg(target_os = "windows")]
fn get_l2_cache_kb() -> Option<u64> {
    use wmi::{COMLibrary, WMIConnection};
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(rename = "Win32_Processor")]
    #[serde(rename_all = "PascalCase")]
    struct Win32Processor { l2_cache_size: Option<u32> }
    let com = COMLibrary::new().ok()?;
    let wmi = WMIConnection::new(com).ok()?;
    let results: Vec<Win32Processor> = wmi.query().ok()?;
    results.into_iter().next()?.l2_cache_size.map(|v| v as u64)
}

#[cfg(target_os = "windows")]
fn get_l3_cache_kb() -> Option<u64> {
    use wmi::{COMLibrary, WMIConnection};
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(rename = "Win32_Processor")]
    #[serde(rename_all = "PascalCase")]
    struct Win32Processor { l3_cache_size: Option<u32> }
    let com = COMLibrary::new().ok()?;
    let wmi = WMIConnection::new(com).ok()?;
    let results: Vec<Win32Processor> = wmi.query().ok()?;
    results.into_iter().next()?.l3_cache_size.map(|v| v as u64)
}

// ── Linux ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn get_physical_cores() -> Option<u32> {
    use std::fs;
    let content = fs::read_to_string("/proc/cpuinfo").ok()?;
    for line in content.lines() {
        if line.starts_with("cpu cores") {
            return line.split(':').nth(1)?.trim().parse().ok();
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn get_boost_clock() -> Option<f64> {
    use std::fs;
    // cpufreq scaling_max_freq is governor-capped, not hardware boost spec.
    // Better: /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq
    let path = "/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq";
    let val: u64 = fs::read_to_string(path).ok()?.trim().parse().ok()?;
    Some(val as f64 / 1000.0) // kHz → MHz
}

#[cfg(target_os = "linux")]
fn get_l2_cache_kb() -> Option<u64> {
    read_cache_size_linux("index2")
}

#[cfg(target_os = "linux")]
fn get_l3_cache_kb() -> Option<u64> {
    read_cache_size_linux("index3")
}

#[cfg(target_os = "linux")]
fn read_cache_size_linux(index: &str) -> Option<u64> {
    use std::fs;
    let path = format!("/sys/devices/system/cpu/cpu0/cache/{}/size", index);
    let raw = fs::read_to_string(path).ok()?;
    let raw = raw.trim();
    if raw.ends_with('K') {
        raw[..raw.len()-1].parse().ok()
    } else if raw.ends_with('M') {
        let mb: u64 = raw[..raw.len()-1].parse().ok()?;
        Some(mb * 1024)
    } else {
        raw.parse().ok()
    }
}

// ── macOS ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn get_physical_cores() -> Option<u32> {
    let output = std::process::Command::new("sysctl")
        .args(["-n", "hw.physicalcpu"])
        .output().ok()?;
    String::from_utf8(output.stdout).ok()?.trim().parse().ok()
}

#[cfg(target_os = "macos")]
fn get_boost_clock() -> Option<f64> {
    None // macOS does not expose boost clock via public API
}

#[cfg(target_os = "macos")]
fn get_l2_cache_kb() -> Option<u64> {
    sysctl_cache_mac("hw.l2cachesize")
}

#[cfg(target_os = "macos")]
fn get_l3_cache_kb() -> Option<u64> {
    sysctl_cache_mac("hw.l3cachesize")
}

#[cfg(target_os = "macos")]
fn sysctl_cache_mac(key: &str) -> Option<u64> {
    let output = std::process::Command::new("sysctl")
        .args(["-n", key])
        .output().ok()?;
    let bytes: u64 = String::from_utf8(output.stdout).ok()?.trim().parse().ok()?;
    Some(bytes / 1024) // bytes → KB
}
