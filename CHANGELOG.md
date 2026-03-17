# Changelog

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
