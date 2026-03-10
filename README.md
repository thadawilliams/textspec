# textspec

**Clean hardware spec snapshots for posting anywhere.**

textspec collects your system specs and formats them as clean plain text — ready to paste into Reddit, Discord, forums, or anywhere else you want to share your build.

No installation. No dependencies. Just download and run.

---

## Download

Go to the [Releases](../../releases) page and download the latest binary for your platform:

| Platform | File |
|---|---|
| Windows 64-bit | `textspec-windows-x86_64.exe` |

> **Windows note:** Windows Defender may show a warning when you first run the file since it's unsigned. Click **More info → Run anyway** to proceed. This is normal for indie tools that haven't been through Microsoft's paid signing process.

---

## Usage

### Double-click
Just double-click `textspec-windows-x86_64.exe`. A window will open, collect your specs, display them, allowing you to copy them, and wait for you to press Enter before closing.

### From the command line
```
textspec-windows-x86_64.exe
```

### Save output to a file
```
textspec-windows-x86_64.exe --output myspecs.txt
```

### Copy output to clipboard automatically
```
textspec-windows-x86_64.exe --copy
```

### Check version
```
textspec-windows-x86_64.exe --version
```

---

## Example output

```
=== SYSTEM SPECS ===

[ OS ]
  Windows 11 Pro (Build 26200)

[ CPU ]
  AMD Ryzen 7 9800X3D 8-Core Processor
  Cores / Threads: 8 / 16
  Base Clock:  4.70 GHz
  L2 Cache:    8 MB
  L3 Cache:    96 MB

[ MOTHERBOARD ]
  ASRock X870E Nova WiFi
  BIOS: American Megatrends International, LLC. 3.30 (2025-06-16)

[ RAM ]
  Total: 32 GB
  Slot P0 CHANNEL A / DIMM 1: 16 GB DDR5 UD5-6000 @ 6000MHz
  Slot P0 CHANNEL B / DIMM 1: 16 GB DDR5 UD5-6000 @ 6000MHz

[ GPU ]
  AMD Radeon RX 9060 XT
  VRAM: 16 GB (15 GB usable)
  Driver: 32.0.23027.2005

[ INTEGRATED GRAPHICS ]
  AMD Radeon(TM) Graphics

[ DISPLAYS ]
  Display 1: G274QPF E2 (Primary)
    2048x1152 @ 180Hz
  Display 2: LG FHD
    1920x1080 @ 60Hz

[ STORAGE ]
  SATA SSD | Samsung SSD 860 EVO 500GB | 466 GB
  HDD | TOSHIBA HDWG740UZSVC | 3726 GB
  NVMe SSD | SHPP41-2000GM | 1863 GB
  NVMe SSD | SK hynix BC501 HFM256GDJTNG-8310A | 238 GB
  NVMe SSD | INTEL SSDPEKNW010T8 | 954 GB

[ PERIPHERALS ]
  Beats Studio Buds
  Fosi Audio K5 Pro
  Realtek USB Audio
  Yeti Stereo Microphone

===================
```

---

## Notes

- **Peripherals** shows USB and Bluetooth audio devices, keyboards, and mice that Windows exposes directly. Gaming peripherals managed by vendor software (Corsair iCUE, Razer Synapse, Logitech G Hub, etc.) may not appear — those vendors hide their devices behind a virtual layer. You can always add them manually when pasting your spec.

- **VRAM** shows both the card's rated capacity and the amount Windows reports as usable. The usable figure is typically slightly less due to firmware reservation — this is normal.

- **Storage** types (NVMe/SATA/HDD) are detected via Windows' authoritative storage API, not guesswork.

- **Monitor names** are read from EDID data and matched to the correct display using the Windows Display Configuration API. Names reflect what your monitor reports in its firmware.

---

## Platform support

| Platform | Status |
|---|---|
| Windows 10/11 (x86_64) | ✅ Supported |
| Linux | 🚧 In progress |
| macOS | 🚧 In progress |

---

## Feedback

Found a bug or have a suggestion? Open an issue on the [Issues](../../issues) page. If a device isn't being detected correctly, include the output of:

```powershell
Get-CimInstance Win32_PnPEntity | Where-Object { $_.PNPClass -in @('HIDClass','Media','Keyboard','Mouse') } | Select-Object Name, Manufacturer, PNPClass | Format-Table -AutoSize
```

---

## License

textspec is donationware. Free to use. If it's useful to you, consider buying me a coffee — it helps fund continued development.

☕ [buymeacoffee.com/thadawilliams](https://buymeacoffee.com/thadawilliams)

Built with Rust.
