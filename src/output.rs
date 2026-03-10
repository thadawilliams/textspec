use crate::models::*;

/// Formats the full system snapshot as clean plain text
/// suitable for pasting into Reddit, Discord, forums, etc.
pub fn format_snapshot(snap: &SystemSnapshot) -> String {
    let mut out = String::new();

    out.push_str("=== SYSTEM SPECS ===\n\n");

    // OS
    if let Some(os) = &snap.os {
        out.push_str("[ OS ]\n");
        let build = os.build.as_deref().map(|b| format!(" (Build {})", b)).unwrap_or_default();
        out.push_str(&format!("  {}{}\n\n", os.name, build));
    }

    // CPU
    if let Some(cpu) = &snap.cpu {
        out.push_str("[ CPU ]\n");
        out.push_str(&format!("  {}\n", cpu.brand));
        out.push_str(&format!("  Cores / Threads: {} / {}\n", cpu.cores, cpu.threads));
        out.push_str(&format!("  Base Clock:  {:.2} GHz\n", cpu.base_clock_mhz / 1000.0));
        if let Some(boost) = cpu.boost_clock_mhz {
            out.push_str(&format!("  Boost Clock: {:.2} GHz\n", boost / 1000.0));
        }
        if let Some(l2) = cpu.cache_l2_kb {
            out.push_str(&format!("  L2 Cache:    {}\n", format_cache(l2)));
        }
        if let Some(l3) = cpu.cache_l3_kb {
            out.push_str(&format!("  L3 Cache:    {}\n", format_cache(l3)));
        }
        out.push('\n');
    }

    // Motherboard
    if let Some(mb) = &snap.motherboard {
        out.push_str("[ MOTHERBOARD ]\n");
        out.push_str(&format!("  {} {}\n", mb.manufacturer, mb.model));
        if mb.bios_version.is_some() || mb.bios_date.is_some() {
            let ver = mb.bios_version.as_deref().unwrap_or("?");
            let date = mb.bios_date.as_deref().unwrap_or("?");
            let vendor = mb.bios_vendor.as_deref().unwrap_or("");
            out.push_str(&format!("  BIOS: {} {} ({})\n", vendor, ver, date));
        }
        out.push('\n');
    }

    // RAM
    if !snap.ram.is_empty() {
        out.push_str("[ RAM ]\n");
        let total_mb: u64 = snap.ram.iter().map(|r| r.capacity_mb).sum();
        out.push_str(&format!("  Total: {}\n", format_ram_size(total_mb)));

        for stick in &snap.ram {
            let slot = stick.slot.as_deref().unwrap_or("?");
            let mfr = stick.manufacturer.as_deref().unwrap_or("");
            let part = stick.part_number.as_deref().unwrap_or("");
            let mem_type = stick.memory_type.as_deref().unwrap_or("DDR");
            let speed = stick.speed_mhz.map(|s| format!(" @ {}MHz", s)).unwrap_or_default();
            let timings = format_timings(stick);

            // Build the identifier — part number if we have it, manufacturer if we have it
            let ident = match (mfr, part) {
                ("", "") => String::new(),
                ("", p)  => format!(" {}", p),
                (m, "")  => format!(" {}", m),
                (m, p)   => format!(" {} {}", m, p),
            };

            out.push_str(&format!(
                "  Slot {}: {} {}{}{}{}\n",
                slot,
                format_ram_size(stick.capacity_mb),
                mem_type,
                ident,
                speed,
                timings,
            ));
        }
        out.push('\n');
    }

    // GPUs — discrete first, then integrated
    let discrete: Vec<&GpuInfo> = snap.gpus.iter().filter(|g| !g.is_integrated).collect();
    let integrated: Vec<&GpuInfo> = snap.gpus.iter().filter(|g| g.is_integrated).collect();

    if !discrete.is_empty() {
        out.push_str("[ GPU ]\n");
        for gpu in &discrete {
            out.push_str(&format!("  {}\n", gpu.name));
            if let Some(vram) = gpu.vram_mb {
                out.push_str(&format!("  VRAM: {}\n", format_ram_size(vram)));
            }
            if let Some(driver) = &gpu.driver_version {
                out.push_str(&format!("  Driver: {}\n", driver));
            }
        }
        out.push('\n');
    }

    if !integrated.is_empty() {
        out.push_str("[ INTEGRATED GRAPHICS ]\n");
        for gpu in &integrated {
            out.push_str(&format!("  {}\n", gpu.name));
            if let Some(shared) = gpu.shared_memory_mb {
                out.push_str(&format!("  Shared Memory: {}\n", format_ram_size(shared)));
            }
        }
        out.push('\n');
    }

    // Displays
    if !snap.displays.is_empty() {
        out.push_str("[ DISPLAYS ]\n");
        for (i, display) in snap.displays.iter().enumerate() {
            let primary = if display.is_primary { " (Primary)" } else { "" };
            let name = display.name.as_deref().unwrap_or("Unknown Monitor");
            let refresh = display
                .refresh_rate_hz
                .map(|r| format!(" @ {}Hz", r as u32))
                .unwrap_or_default();
            out.push_str(&format!(
                "  Display {}: {}{}\n",
                i + 1,
                name,
                primary
            ));
            out.push_str(&format!(
                "    {}x{}{}\n",
                display.resolution_w, display.resolution_h, refresh
            ));
        }
        out.push('\n');
    }

    // Storage
    if !snap.storage.is_empty() {
        out.push_str("[ STORAGE ]\n");
        for device in &snap.storage {
            let kind = match device.device_type {
                StorageType::NvmeSsd => "NVMe SSD",
                StorageType::SataSsd => "SATA SSD",
                StorageType::Hdd     => "HDD",
                StorageType::Unknown => "Storage",
            };
            // Only show interface label when it adds info beyond the type
            // e.g. show USB, ATA — skip NVMe/SATA since the type already says that
            let iface = device.interface.as_deref()
                .filter(|i| !matches!(*i, "NVMe" | "SATA" | "SCSI" | "IDE"))
                .map(|i| format!(" [{}]", i))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {} | {} | {:.0} GB{}\n",
                kind, device.model, device.capacity_gb, iface
            ));
        }
        out.push('\n');
    }

    out.push_str("===================\n");
    out
}

fn format_cache(kb: u64) -> String {
    if kb >= 1024 {
        format!("{} MB", kb / 1024)
    } else {
        format!("{} KB", kb)
    }
}

fn format_ram_size(mb: u64) -> String {
    if mb >= 1024 {
        format!("{} GB", mb / 1024)
    } else {
        format!("{} MB", mb)
    }
}

fn format_timings(stick: &RamStick) -> String {
    match (stick.cas_latency, stick.trcd, stick.trp, stick.tras) {
        (Some(cl), Some(trcd), Some(trp), Some(tras)) => {
            format!(" CL{}-{}-{}-{}", cl, trcd, trp, tras)
        }
        (Some(cl), _, _, _) => format!(" CL{}", cl),
        _ => String::new(),
    }
}
