# Changelog

## 2025-03-17 – Phases 2–6: Full Roadmap Implementation

### Summary
Implemented Phases 2–6 of the TS-OS Full Development Roadmap: memory protection (paging), persistent storage (disk), usable shell (cd, pwd, rm, pipes, redirection, history), multi-process (getpid, chdir, getcwd, wait, kill), and ecosystem (wc, head, tail, help &lt;cmd&gt;).

### Phase 2: Memory Protection
- **4-level paging** – Per-process page tables, CR3 switch on schedule
- **Syscall validation** – Bounds-check user pointers for all syscalls
- **ProcessNode** – Added cr3 field, page table creation on spawn

### Phase 3: Persistent Storage
- **IDE PIO driver** – disk.rs, read_sector/write_sector (primary master 0x1F0)
- **Checkpoint to disk** – persist.rs writes to sector 0, restore from disk on boot
- **disk.img** – 16 MB, created by make run

### Phase 4: Usable Shell
- **cd, pwd** – Working directory with resolve_path
- **rm** – Remove file or empty directory (fs::rm, SYS_RM)
- **Pipes** – cmd1 | cmd2 (e.g. ls | cat out.txt)
- **Redirection** – cmd > file, cmd < file
- **History** – Up/down arrows, 16 entries
- **Line editor** – Backspace, arrow keys (ANSI escape sequences)
- **Keyboard** – Arrow scancodes, backspace

### Phase 5: Multi-Process
- **getpid, chdir, getcwd** – Per-process cwd in ProcessNode
- **wait** – Block until child exits, return pid and status
- **kill** – SYS_KILL(pid, 9) for SIGKILL
- **Parent-child** – ProcessNode.parent, exit_status, NodeState::Waiting

### Phase 6: Ecosystem
- **wc, head, tail** – Minimal text utilities
- **help &lt;cmd&gt;** – Per-command help

---

## 2025-03-17 – Phase 1: Stability Foundation (Partial)

### Summary
Phase 1 of the TS-OS Full Development Roadmap. Fixed boot crash (allocator), added Limine framebuffer VGA driver, removed debug instrumentation. Boot is stable; VGA output does not yet appear in QEMU window (serial-only).

---

### Allocator
- **Heap increased** – 64 KiB → 256 KiB (bump allocator)
- **Boot crash fixed** – ALLOC ERROR no longer occurs during boot

### VGA
- **Limine framebuffer** – Added `FramebufferRequest` and `HhdmRequest` to limine requests
- **vga.rs** – New framebuffer text driver: 8×8 font (font8x8_basic), 80×25 text grid, scroll, clear
- **Address handling** – HHDM applied only when framebuffer address < 4 GB (physical); addresses ≥ 4 GB used as-is
- **Status** – Framebuffer received (e.g. 1280×800); display does not appear in QEMU window; serial output works

### Debug
- **Removed** – `dbg: before GRAPH`, `dbg: before add_node 0`, `dbg: before init_node_stacks`, `dbg: before Vec Heap OK`
- **Simplified** – `alloc_error` now prints only `ALLOC ERR` (no layout details)
- **Removed** – `serial_write_u32` (was dead code after alloc_error change)
- **Added** – Serial debug for framebuffer: `FB: 0x... -> 0x... WxH` on boot

### README
- Updated status: 256 KiB heap, VGA framebuffer implementation, serial-only output
- Remaining work: VGA display in QEMU, Phase 2+ items

---

### Current capabilities
- Boots with Limine (BIOS + UEFI)
- Serial output (primary), PS/2 keyboard
- Strongest Node scheduler with dynamic emergence (up to 32 nodes)
- User-mode shell (help, ps, echo, spawn, ls, cat, touch, mkdir, shutdown)
- In-RAM filesystem with checkpoint/restore

### Known limitations
- VGA framebuffer does not display in QEMU window (serial-only)
- No paging / no process isolation
- Bump allocator (no free)
- Persistence in-RAM only (lost on power cycle)

---

## 2025-03-17 – Post-cleanup build fix

### Summary
Fixed all compilation errors and restored `make all` after removing `paging.rs` and `allocator.rs`. The kernel now builds cleanly on Windows (MSYS) and Linux without external C compiler.

---

### Build system
- **Removed `cc` dependency** – No longer assembles `idt.S` via external C compiler
- **Removed `src/idt.S`** – Assembly moved inline to Rust
- **`build.rs`** – Dropped idt compilation; only sets linker script

### Kernel (`kernel/src/main.rs`)
- **Paging removed** – Dropped `cr3` from `ProcessNode`, removed `paging::switch_cr3` from scheduler
- **Heap** – Switched from `LinkedListAllocator` to `BumpAllocator`
- **GDT/IDT** – `append` → `add_entry` (x86_64 0.14.13 API); `TaskStateSegment::new()` instead of struct literal
- **VirtAddr** – Use `timer_stub as *const ()` / `syscall_stub as *const ()` for IDT handlers
- **GDT/IDT lifetime** – `Box::leak(Box::new(...))` so they live for `'static` and satisfy `load()`
- **Borrow checker** – Copy `neighbors` in `spread_from` before iterating to avoid overlapping borrows
- **Unsafe** – Wrapped `CURRENT_NODE_IDX`, `stack.as_ptr().add()`, and stack init in `unsafe` blocks
- **Assembly** – Inlined `timer_stub` and `syscall_stub` via `core::arch::global_asm!()` (Intel syntax)
- **Warnings** – Added `#![allow(dead_code, static_mut_refs)]` at crate level

### Allocator (`kernel/src/allocator.rs`)
- **Recreated** – Simple bump allocator (64 KiB, no free)

### Filesystem (`kernel/src/fs.rs`)
- **FsNode** – Added `#[derive(Clone, Copy)]` and `[const { FsNode::empty() }; MAX_NODES]` for static init
- **ToString** – Added `use alloc::string::ToString`
- **alloc_node** – Replaced `ok_or(()).unwrap_or(return false)` with `match` for clarity
- **resolve** – Removed unnecessary `unsafe` block

### Shell (`kernel/src/shell.rs`)
- **Imports** – Removed unused `alloc::string::String`
- **Lifetime** – `parse_echo_args` now returns `Option<EchoResult<'_>>`

### Dependencies (`kernel/Cargo.toml`)
- **x86_64** – Enabled `instructions` feature for GDT/IDT load
- **cc** – Removed from build-dependencies

### README
- Updated status: paging removed, bump allocator, no process isolation
- Build requirements: no C compiler needed (assembly inlined)

---

### Current capabilities
- Boots with Limine (BIOS + UEFI)
- VGA + serial output, PS/2 keyboard
- Strongest Node scheduler with dynamic emergence (up to 32 nodes)
- User-mode shell (help, ps, echo, spawn, ls, cat, touch, mkdir, shutdown)
- In-RAM filesystem with checkpoint/restore

### Known limitations
- No paging / no process isolation
- Bump allocator (no free)
- Persistence in-RAM only (lost on power cycle)
