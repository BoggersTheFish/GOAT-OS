# BTFOS Hardware Support

## Supported platform

- **Architecture**: x86, 32-bit protected mode.
- **Tested on**: QEMU (`qemu-system-i386` / `qemu-system-x86`). CI runs on Ubuntu with QEMU.

## Dependency verification

Before building or running, verify tools:

- **Linux/WSL**: `./scripts/check_deps.sh`
- **Windows**: `.\scripts\check_deps.ps1`

Required:

| Tool | Purpose |
|------|--------|
| **nasm** | Assemble boot sector |
| **gcc** (32-bit) | Compile kernel (`-m32` or multilib) |
| **qemu-system-i386** or **qemu-system-x86** | Run and verify boot |

Optional for ISO: `grub-mkrescue` (or `grub2-mkrescue`), **xorriso**.

## Real hardware (x86 only)

BTFOS is **not validated on physical x86 hardware**. If you boot from the generated ISO on real hardware:

1. **Boot**: Use the built ISO (`make iso` → `btfos.iso`). Boot via BIOS/UEFI (CSM/legacy) as a Multiboot kernel. GRUB loads the kernel; no drivers beyond VGA and serial.
2. **Drivers**: Only stubs and in-kernel VGA (0xB8000) and COM1 (0x3F8). No disk, network, or USB drivers.
3. **32-bit only**: No long mode; single core assumed for scheduling. Graph shards are logical only.

Report hardware results (or 404/build issues) via [GitHub Issues](https://github.com/BoggersTheFish/GOAT-OS/issues).
