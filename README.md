# BTFOS (BoggersTheFish OS) / GOAT-OS

Minimal, graph-driven OS with a GOAT-TS–inspired kernel: cognition loop, activation/decay, forces, curiosity, reflection, and self-assessment. Targets x86 (32-bit protected mode), runs under QEMU.

**MIT License**

## Features

- **Kernel (C)**: GOAT-TS-style graph (nodes = processes/pages/syscalls, edges = dependencies). Activation spread, decay, forces (affinity), curiosity (idle reuse), reflection (tension/bottleneck), goal-generation.
- **Boot**: Multiboot 1 (GRUB-compatible); `boot.asm` (NASM) or `boot.s` (GAS). **Bootable ISO** via `make iso`.
- **Memory**: Graph-based page states (ACTIVE/DORMANT/DEEP).
- **Scheduler**: Cognition loop every tick; configurable forces and reflection.
- **System calls**: fork, exec, read, write, open, close, exit, getpid, yield, gettime (stubs; all ingested into graph).
- **FS**: In-memory triple store (ingest as text); mkdir, stat; root, /boot, /tmp.
- **Self-improvement**: Self-assess scans kernel tag, emits hypotheses (e.g. optimize scheduler when tension > 0.5).
- **Scaling**: Graph shards per logical core. **Low-power preset** for weak CPUs (e.g. Pentium Silver).
- **Plugins**: Discoverable modules (e.g. driver_vga, driver_serial).
- **Monitoring**: JSON stats on serial.
- **Shell**: help, status, run, exit, bench, stat &lt;path&gt;.

## Verify dependencies and boot

- **Linux/WSL**: `./scripts/check_deps.sh` then `./scripts/verify_boot.sh` (builds, runs QEMU, asserts "BTFOS Ready").
- **Windows**: `.\scripts\check_deps.ps1` then `.\scripts\verify_boot.ps1` (or use `build.ps1` and run QEMU manually).

CI (GitHub Actions) runs the same: build kernel, build ISO, run QEMU, and **fail if "BTFOS Ready" is not seen**—so the repo is not a skeleton; it is build- and boot-verified.

## Build

- **Linux/WSL**: `make` (needs `nasm`, `gcc` with 32-bit support).
- **Windows**: MinGW/MSYS2 with `nasm` and `gcc -m32`, or WSL. Or `.\build.ps1`.

```bash
make
make iso   # optional: bootable btfos.iso (needs grub-mkrescue)
```

## Run (QEMU)

```bash
make run
# or
./run_qemu.sh
# Windows: run_qemu.bat
# From ISO: qemu-system-i386 -cdrom btfos.iso -serial stdio
```

Success: serial (and optionally VGA) shows **"BTFOS Ready"**, then shell and JSON logs.

## Config presets

In `include/btfos_config.h`:

- `BTFOS_BOOT`: light (few ticks, no forces/reflection).
- `BTFOS_LOW_POWER`: fewer graph nodes/edges, no reflection (for low-power CPUs).
- `BTFOS_NORMAL`: default (forces, reflection, curiosity, goals, 2 shards).
- `BTFOS_FULL`: more ticks, 4 shards.
- `BTFOS_BENCHMARK`: fixed 10k ticks + JSON output for measuring (see [docs/BENCHMARKS.md](docs/BENCHMARKS.md)).

## Benchmarks and hardware

- **Benchmarks**: No unsubstantiated performance claims. Use the benchmark preset and measure ticks/sec; see [docs/BENCHMARKS.md](docs/BENCHMARKS.md).
- **Hardware**: x86 32-bit only; tested in QEMU. Real hardware: see [docs/HARDWARE.md](docs/HARDWARE.md).

## Scope and roadmap

Minimal FS and syscalls; **not production-ready** (no networking, no real device drivers). See [docs/ROADMAP.md](docs/ROADMAP.md) for next steps (persistent FS, 64-bit, drivers, etc.).

## Community

- **Issues & PRs**: [GitHub Issues](https://github.com/BoggersTheFish/GOAT-OS/issues) and [Pull requests](https://github.com/BoggersTheFish/GOAT-OS/pulls). Report 404s, build failures, or missing code so we can fix them.
- **Contributing**: [CONTRIBUTING.md](CONTRIBUTING.md). Good first areas: shell commands, benchmark presets, CI, docs.
- **Code of conduct**: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Layout

- `boot/` – Multiboot bootstrap (NASM/GAS).
- `kernel/kernel.c` – Kernel + graph, scheduler, memory, syscalls, FS, shell, self-assess, plugins.
- `include/btfos_config.h` – Presets and limits.
- `iso/boot/grub/grub.cfg` – GRUB config for `make iso`.
- `scripts/` – `check_deps.*`, `verify_boot.*`.
- `docs/` – HARDWARE.md, BENCHMARKS.md, ROADMAP.md.
- `.github/workflows/build-and-boot.yml` – CI build and boot verification.

Experimental, modular. No external deps beyond compiler and NASM.
