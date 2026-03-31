# Command-Line Development Tool User Guide

- **Version:** 1.0
- **Author:** liuyi@rock-chips.com
- **Date:** 2021-08-26
- **Classification:** Public

## Overview

The command-line development tool provides developers with firmware flashing, image flashing, device erasing, device switching, and storage switching capabilities.

### Supported Chips

3308, 3326, 3399, 3328, 3228H, 3229, 3368, 3228, 3288, 3128, 3126, 3188, 3036, 1808, PX30, 1109, 1126, 3566, 3568

### Target Audience

This document is intended for developers.

---

## 1. Common Functions

### 1.1 List Upgrade Devices

```
upgrade_tool ld
```

Upgradeable devices are collectively referred to as **Rockusb**. There are two modes: **Loader** and **Maskrom**.

### 1.2 Download Boot

When the device is in Maskrom mode, you must download Boot before communication is possible. The Boot download process does **not** write to device storage.

```
upgrade_tool db rkxxloader.bin
```

**Troubleshooting:**
1. Check DDR or SoC. Restart the device before retrying.
2. Check USB connection. Restart the device before retrying.

### 1.3 Flash Loader

This operation can be performed in either Maskrom or Loader mode. Flashing the loader downloads Boot and generates an IDBlock written to device storage.

```bash
# Flash loader and restart device
upgrade_tool ul rkxxloader.bin

# Flash loader without restarting
upgrade_tool ul rkxxloader.bin -noreset

# When multiple storage devices exist, specify the target storage (e.g. SPINOR)
upgrade_tool ul rkxxloader.bin SPINOR
```

**Troubleshooting:**
1. Check DDR or SoC. Restart the device before retrying.
2. Check USB connection. Restart the device before retrying.
3. Check if Flash is on the supported list, or check for cold solder joints.
4. Communication error — if intermittent, check USB connection; if persistent, check device side.

### 1.4 Flash Partition Images

**Note:** You must flash the partition table before flashing partition images.

```bash
# Flash partition table
upgrade_tool di -p parameter.txt

# Flash a single partition image
upgrade_tool di -k kernel.img

# Flash multiple partition images
upgrade_tool di -u uboot.img -b boot.img

# Flash a partition without a predefined shortcut (e.g. vendor)
upgrade_tool di -vendor vendor.img

# For A/B partitions (e.g. boot_a and boot_b)
upgrade_tool di -boot_a boot.img -boot_b boot.img
```

**Predefined partition shortcuts:**
| Flag | Partition |
|------|-----------|
| `-b` | boot |
| `-k` | kernel |
| `-r` | recovery |
| `-s` | system |
| `-u` | uboot |
| `-m` | misc |
| `-t` | trust |

**Troubleshooting:**
1. Check if the image file exists or is locked by another process.
2. Partition definition is too small for the image.
3. Check USB connection. Restart the device before retrying.

### 1.5 Device Switching

```bash
# Switch from Loader mode to Maskrom mode
upgrade_tool rd 3
```

### 1.6 Upgrade Firmware

This operation can be performed in either Maskrom or Loader mode. The process downloads Boot automatically — no need to download Boot beforehand.

```bash
# Flash upgrade firmware and restart device
upgrade_tool uf update.img

# Flash upgrade firmware without restarting
upgrade_tool uf update.img -noreset
```

**Troubleshooting:**
1. Check if the firmware file exists or is locked.
2. Firmware identifier error — check the firmware packaging process.
3. Firmware digest check failed — confirm the firmware has not been modified.
4. Firmware read failed — update the firmware packaging tool and regenerate.
5. A partition in the firmware is too small for its image.
6. Incorrect chip identifier in firmware — read chip info with the tool, then regenerate firmware.
7. Communication error — if intermittent, check USB connection; if persistent, check device side.

### 1.7 Read/Write Files by Address

```bash
# Write a file to LBA address 0x12000
upgrade_tool wl 0x12000 oem.img

# Read data from address 0x12000, length 0x2000 sectors, save to out.img
upgrade_tool rl 0x12000 0x2000 out.img
```

### 1.8 Device Erase

```bash
# Erase all data on storage — execute in Maskrom mode, no Boot download needed
upgrade_tool ef rkxxloader.bin

# Erase by sector address (eMMC only) — erase 0x2000 sectors starting from sector 0
upgrade_tool el 0 0x2000
```

### 1.9 Read Device Information

```bash
# Read storage info
upgrade_tool rfi

# Read chip ID
upgrade_tool rci

# Read partition table
upgrade_tool pl
```

### 1.10 Multi-Storage Operations

When multiple storage devices are present, you can switch between them. This must be done in Maskrom mode after a successful Boot download.

```
upgrade_tool ssd
```

After running the command, available storage devices are listed. The one marked with `*` is the current storage. Enter the `No` of the desired storage to select it.

### 1.11 Multi-Device Selection

When multiple devices are connected, first list all devices with `ld`, note the `LocationID` of the target device, then use the `-s` parameter to specify it.

```bash
# Upgrade loader on the device with LocationID 100
upgrade_tool -s 100 ul rkxxloader.bin
```
