# BTFOS (BoggersTheFish OS)

Minimal, graph-driven OS with a GOAT-TS–inspired kernel: cognition loop, activation/decay, forces, curiosity, reflection, and self-assessment. Targets x86 (32-bit protected mode), runs under QEMU.

**MIT License**

## Features

- **Kernel (C)**: GOAT-TS-style graph (nodes = processes/pages/syscalls, edges = dependencies). Activation spread, decay, forces (affinity), curiosity (idle reuse), reflection (tension/bottleneck), goal-generation.
- **Boot**: Multiboot 1 (GRUB-compatible); `boot.asm` (NASM) or `boot.s` (GAS).
- **Memory**: Graph-based page states (ACTIVE/DORMANT/DEEP).
- **Scheduler**: Cognition loop every tick; configurable forces and reflection.
- **System calls**: fork/exec/read/write stubs; ingested as graph for provenance.
- **FS**: In-memory triple store (ingest files as text triples).
- **Self-improvement**: Self-assess scans kernel tag, emits hypotheses (e.g. optimize scheduler when tension > 0.5).
- **Scaling**: Graph shards per logical core.
- **Plugins**: Discoverable modules (e.g. driver_vga, driver_serial).
- **Monitoring**: JSON stats on serial.
- **Shell**: Ingests commands, reasons over outputs (help, status, run, exit).

## Build

- **Linux/WSL**: `make` (needs `nasm`, `gcc` with 32-bit support, `ld`).
- **Windows**: Use MinGW/MSYS2 with `nasm` and `gcc -m32`, or WSL. Then `make` or `mingw32-make`.

```bash
make
```

## Run (QEMU)

```bash
make run
# or
./run_qemu.sh
# Windows:
run_qemu.bat
```

Success: serial (and optionally VGA) shows **"BTFOS Ready"**, then shell and JSON logs.

## Config presets

In `include/btfos_config.h`:

- `BTFOS_BOOT`: light (few ticks, no forces/reflection).
- `BTFOS_NORMAL`: default (forces, reflection, curiosity, goals, 2 shards).
- `BTFOS_FULL`: more ticks, 4 shards.

## Layout

- `boot/boot.asm` – Multiboot + bootstrap (NASM).
- `boot/boot.s` – Same in GAS.
- `kernel/kernel.c` – Kernel + graph, scheduler, memory, syscalls, FS, shell, self-assess, plugins.
- `kernel/linker.ld` – Linker script.
- `include/btfos_config.h` – Presets and limits.

Experimental, modular, vibe-coded. No external deps beyond compiler and NASM.
