# Changelog

All notable changes to BTFOS / GOAT-OS are recorded here. Update this file with each push.

Format: **[Update N]** – short title, then detailed list.

---

## [Update 2] – Kernel compilation fixes, single-file build

**Date:** 2025-03

- **Kernel single-file refactor**
  - Removed all includes of non-existent headers (`kernel.h`, `process.h`, `memory.h`, `fs.h`, `shell.h`, `graph.h`, `cognition.h`, `reasoning.h`, `monitor.h`). Kernel now compiles with only `btfos_config.h` and standard headers (`stdint.h`, `stddef.h`).
  - Implemented all previously external symbols inside `kernel.c`: graph (create/add_node/add_edge, spread_activation, decay_states, apply_forces, curiosity, reflection, goal_generator, sandbox_test, get_activation/get_state, destroy), process (init, list, `process_t` with next/pid/activation/state/parent), memory_init, FS (triple pool, alloc_triple, insert_triple, init, lookup, mkdir, ingest_file), shell (init, run, ingest_command), monitor_print (serial + VGA).
- **Boot entry**
  - `kernel_main` signature fixed to `kernel_main(uint32_t magic, uint32_t mb_info)` to match `boot.asm` (Multiboot). Added magic check `0x2BADB002`.
- **Config**
  - In `include/btfos_config.h`: added `BTFOS_BOOT_LIGHT`, `BTFOS_BOOT_NORMAL`, `BTFOS_BOOT_FULL` and `BTFOS_BOOT_PRESET` so cognition preset comparisons compile.
- **Code quality**
  - Fixed misleading indentation and variable shadowing in `fs_mkdir`, `fs_ingest_file`, `shell_ingest_command` (braced `for` loops; renamed `t` → `ftype` / `pred_content` where needed).
  - `fs_lookup` declared before use and defined in-file; no implicit declaration.
  - Syscall stubs: added `(void)param` for unused parameters to satisfy `-Wall -Wextra`.
  - Removed commented-out unused `shell_buf` / `shell_len`.
  - Graph node pool reset on each `graph_create()` so cognition loop does not accumulate nodes across iterations.
- **Build**
  - Compiles with `gcc -m32 -ffreestanding -fno-stack-protector -fno-pie -Wall -Wextra -I include -O1 -c kernel/kernel.c` (or `make`) with no errors.

---

## [Update 1] – CI, ISO, docs, community, scope

**Date:** 2025-03

- **Verification & CI**
  - GitHub Actions workflow: build kernel, build bootable ISO, run QEMU, assert "BTFOS Ready" in serial output; upload ISO artifact.
  - Scripts: `scripts/check_deps.sh`, `scripts/check_deps.ps1`, `scripts/verify_boot.sh`, `scripts/verify_boot.ps1` for dependency checks and boot verification.
- **Bootable ISO**
  - `make iso` produces `btfos.iso` (GRUB + kernel). Added `iso/boot/grub/grub.cfg`.
- **Config presets**
  - `BTFOS_LOW_POWER`, `BTFOS_BENCHMARK` presets; low-power reduces graph nodes/edges and disables reflection in hot path.
- **Docs**
  - `docs/HARDWARE.md` (x86 32-bit, QEMU, real hardware notes), `docs/BENCHMARKS.md` (how to run benchmark preset, no unsubstantiated claims), `docs/ROADMAP.md` (scope, not production-ready, next steps).
- **Community**
  - `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `.github/ISSUE_TEMPLATE/bug_report.md`, `.github/ISSUE_TEMPLATE/feature_request.md`, `.github/PULL_REQUEST_TEMPLATE.md`. README Community section with links to issues/PRs.
- **Scope**
  - Extended syscalls (open, close, exit, getpid, yield, gettime); FS mkdir/stat; shell commands bench, stat &lt;path&gt;; ROADMAP for production readiness.

---

*Add new entries at the top under a new `## [Update N]` heading when you push.*
