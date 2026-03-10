/// All hardware data collected from the system.
/// Each field is Option<T> so missing data is handled
/// gracefully rather than failing the whole run.

#[derive(Debug, Default)]
pub struct SystemSnapshot {
    pub cpu: Option<CpuInfo>,
    pub motherboard: Option<MotherboardInfo>,
    pub ram: Vec<RamStick>,
    pub gpus: Vec<GpuInfo>,
    pub displays: Vec<DisplayInfo>,
    pub storage: Vec<StorageDevice>,
    pub os: Option<OsInfo>,
}

#[derive(Debug, Default)]
pub struct CpuInfo {
    pub brand: String,        // e.g. "AMD Ryzen 7 9800X3D"
    pub cores: u32,
    pub threads: u32,
    pub base_clock_mhz: f64,
    pub boost_clock_mhz: Option<f64>,
    pub cache_l2_kb: Option<u64>,
    pub cache_l3_kb: Option<u64>,
}

#[derive(Debug, Default)]
pub struct MotherboardInfo {
    pub manufacturer: String, // e.g. "ASUSTeK"
    pub model: String,        // e.g. "ROG STRIX X670E-F"
    pub bios_vendor: Option<String>,
    pub bios_version: Option<String>,
    pub bios_date: Option<String>,
}

#[derive(Debug, Default)]
pub struct RamStick {
    pub slot: Option<String>,
    pub manufacturer: Option<String>,
    pub part_number: Option<String>,
    pub capacity_mb: u64,
    pub speed_mhz: Option<u32>,
    pub memory_type: Option<String>, // DDR4, DDR5, etc.
    // Timings - these may not always be available without elevated privileges
    pub cas_latency: Option<u32>,    // CL
    pub trcd: Option<u32>,
    pub trp: Option<u32>,
    pub tras: Option<u32>,
}

#[derive(Debug, Default)]
pub struct GpuInfo {
    pub name: String,           // e.g. "AMD Radeon RX 9060 XT"
    pub vram_mb: Option<u64>,
    pub driver_version: Option<String>,
    pub is_integrated: bool,
    // Integrated GPU specific
    pub shared_memory_mb: Option<u64>,
}

#[derive(Debug, Default)]
pub struct DisplayInfo {
    pub name: Option<String>,       // Monitor model if available
    pub resolution_w: u32,
    pub resolution_h: u32,
    pub refresh_rate_hz: Option<f64>,
    pub is_primary: bool,
}

#[derive(Debug, Default)]
pub struct StorageDevice {
    pub model: String,
    pub capacity_gb: f64,
    pub device_type: StorageType,
    pub interface: Option<String>, // NVMe, SATA, USB, etc.
}

#[derive(Debug, Default, PartialEq)]
pub enum StorageType {
    NvmeSsd,
    SataSsd,
    Hdd,
    #[default]
    Unknown,
}

#[derive(Debug, Default)]
pub struct OsInfo {
    pub name: String,           // e.g. "Windows 11 Pro"
    pub build: Option<String>,
}
