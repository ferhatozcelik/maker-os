# Boot and Storage Architecture

[Türkçe](boot-and-storage.md) · [English](boot-and-storage.en.md)

MakerOS uses a read-only EROFS root filesystem and an A/B partition layout to ensure resilience against power loss and interrupted updates. The partition table and update logic are defined in [`os/board/rpi4/genimage.cfg.in`](../../os/board/rpi4/genimage.cfg.in) and [`makerd/src/ota.rs`](../../makerd/src/ota.rs).

## Partition Layout

- **`MOSCFG` (FAT32):** Holds `autoboot.txt` for A/B slot selection and the optional `wifi.txt` preconfiguration file.
- **`BOOT_A` / `BOOT_B` (FAT32):** Kernel (`kernel8.img`), device trees, and board firmware files.
- **`ROOT_A` / `ROOT_B` (EROFS):** Read-only compressed root filesystem slots.
- **`DATA` (ext4):** Persistent storage automatically expanded on first boot (`/home`, `/var`, `/etc` overlay, and user projects).

## A/B OTA Update Flow

Signed `.raucb` update bundles are verified with X.509 certificates and written to the inactive slot. On reboot, the bootloader's `tryboot` mechanism boots the new slot once; once `makeros-mark-good` confirms a healthy boot, the slot becomes permanent—otherwise the bootloader automatically falls back to the previous working slot.
