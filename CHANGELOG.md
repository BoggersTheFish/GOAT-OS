# Changelog

All notable changes to BTFOS / GOAT-OS are recorded here. Update this file with each push.

Format: **[Update N]** – short title, then detailed list.

---

## [Update 6] – PIT ~100ms, syscall triple format (subj=pid, pred=syscall), shell run exec

**Date:** 2025-03

- **PIT timer**
  - Cognition tick interval changed from ~100 Hz to ~10 Hz (~100ms) via PIT divisor 119318 (ports 0x40/0x43).
- **Syscall ingestion**
  - Triples now use subj=pid (decimal), pred="syscall", obj="&lt;name&gt; &lt;detail&gt;" (e.g. "write serial", "exec /path").
- **Shell**
  - Added support for "run exec &lt;task&gt;" in addition to "run &lt;task&gt;" (both call sys_exec).

---

## [Update 5] – Basic syscalls, keyboard input, syscall graph ingestion, interactive shell

**Date:** 2025-03

- **Syscalls (real behavior vs stubs)**
  - `sys_write`: prints to serial+VGA, ingests `(proc:<pid>, write, serial)`, nudges activation.
  - `sys_read`: reads PS/2 keyboard input (scancode polling), ingests `(proc:<pid>, read, kbd)`, nudges activation.
  - `sys_exit`: kills current process in pool; respawns init if PID 1 exits; ingests `(proc:<pid>, exit, "")`.
  - `sys_open/sys_close`: FS stub backed by triple store lookup; ingests open/close triples.
  - `sys_exec`: spawns a process named by path with parent link; ingests `(proc:<childpid>, exec, path)` and prints spawn info.
- **Graph/FS provenance**
  - Added `fs_ingest_syscall(pid, pred, obj)` to store syscall events as triples (`subj=proc:<pid>`), providing provenance signals for cognition.
- **Keyboard**
  - Minimal US keymap and blocking `kbd_read_char_blocking()` for interactive input.
- **Shell**
  - Upgraded to line-based input. Commands: `help`, `status`, `ps`, `run <task>`, `exit`.
- **Scheduling**
  - Timer ticks now increment `ticks`; cognition loop selects the highest-activation runnable process and updates `current_pid`.

---

## [Update 4] – Timer interrupt, cognition per tick, real process_list

**Date:** 2025-03

- **Timer interrupt hook**
  - IDT (256 entries), vector 32 set to timer IRQ stub. PIC remapped to 0x20–0x2F; only IRQ0 unmasked. PIT channel 0 at ~100 Hz (divisor 11932). `boot/timer_isr.asm`: `irq0_entry` saves state, calls C `timer_irq_handler`, EOI, IRET.
  - `timer_irq_handler()` sets `cognition_tick_pending`; main loop runs `cognition_loop(preset)` each tick and updates process priorities/states from graph.
- **Main loop**
  - After `sti`, loop: if `cognition_tick_pending` then clear and run `cognition_loop(preset)`; then `shell_run()`; then `hlt`. Cognition runs periodically from timer, not in a tight loop.
- **Real process_list()**
  - Builds linked list of all active processes in `process_pool` (pid != 0), linked by `.next`; returned list is ingested into graph each cognition tick.
- **Build**
  - `boot/timer_isr.o` added to Makefile; `timer_isr.asm` assembled and linked.

---

## [Update 3] – Graph struct (nodes/edges), cognition loop integration

**Date:** 2025-03

- **Graph implementation**
  - Nodes: `pid`, `activation`, `state`, `mass`, `pos_x`, `pos_y`. Edges: `from_pid`, `to_pid`, `weight` (relations).
  - Functions: `graph_create`, `graph_destroy`, `graph_add_node` (with mass/position), `graph_add_edge` (kind → weight, e.g. "child_of" 0.8), `graph_spread_activation`, `graph_decay_states`, `graph_apply_forces`, `graph_curiosity`, `graph_reflection`, `graph_goal_generator`, `graph_sandbox_test`, `graph_get_activation`, `graph_get_state`. Internal `graph_find_node` for lookups.
  - Spread: push activation along edges then decay. Forces: move node positions toward connected nodes. Curiosity: boost low-activation nodes. Reflection: tension stub.
- **Cognition loop**
  - Ingest processes into graph (add_node with mass/pos, add_edge for parent). Run spread + decay always; apply_forces if preset ≥ NORMAL; curiosity/reflection/goal/sandbox if preset FULL. Write back activation/state to processes.
- **Process**
  - `process_t` extended with `mass`, `pos_x`, `pos_y` for graph integration; bootstrap process defaults 1.0, 0, 0.

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
