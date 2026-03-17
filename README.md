# TS-OS – Strongest Node Operating System (Rust Implementation)

TS-OS is a bare-metal x86_64 microkernel that directly instantiates the **Strongest Node Framework** from [BoggersTheCIG](https://github.com/BoggersTheFish/BoggersTheCIG). The kernel is the Strongest Node; every other component exists only as secondary nodes that dynamically emerge, strengthen, spread activation, detect tension, decay, merge, or get pruned.

**This repo replaces the previous GOAT-OS C prototype.** It is the first full application of the TS framework to the OS domain.

---

## Current Status (TS-State Summary)

| Component | Status |
|-----------|--------|
| **Strongest Node** | Robust TS-OS (activation 100, tension 8) |
| **Paging** | **Removed** – identity map only; no per-process isolation |
| **Heap** | Simple bump allocator (64 KiB, no free) |
| **Keyboard** | Shift, caps lock, 128-byte circular buffer |
| **Persistence** | In-RAM checkpoint (fs + graph count); restore on boot |
| **Process graph** | Vec-based, up to 32 nodes, dynamic emergence |
| **Process isolation** | **None** – all processes share kernel address space |

### Node Table (Example)

| Node | id | activation | tension |
|------|-----|------------|---------|
| 0 | 0 | 100 | 0 |
| 1 | 1 | 80 | 10 |
| 2 | 2 | 60 | 5 |
| 5 | 5 | 50 | 0 |

**Tensions resolved:** No paging, bump allocator, basic keyboard, fixed 32 nodes  
**Tensions remaining:** No process isolation, persistence in-RAM only, no disk, shell minimal  
**Coherence delta:** +4 (post-cleanup iteration)

---

## Implemented Features

### Core
- **Boot with Limine** – x86_64 bare-metal (BIOS + UEFI)
- **GDT + TSS** – Kernel + user segments, TSS for kernel stack
- **Ring 3 user mode** – User CS/SS, iretq, syscall DPL=3
- **Bump heap** – 64 KiB, no free (sufficient for current use)

### Syscalls
| # | Name | Description |
|---|------|-------------|
| 1 | write | stdout → VGA + serial |
| 2 | yield | Yield to scheduler |
| 3 | spawn | Spawn process |
| 4 | read | stdin from keyboard |
| 5 | exit | Exit process |
| 6 | ls | List directory |
| 7 | cat | Read file |
| 8 | ps | List processes |
| 9 | touch | Create file |
| 10 | mkdir | Create directory |
| 11 | write_f | Write to file |
| 12 | shutdown | Checkpoint and halt |

### Scheduler
- **Strongest Node** – activation − tension
- **Spreading activation** – +10 to neighbors
- **Decay** – −2 activation per tick
- **Tension bump** – +1 on preempt
- **Dynamic emergence** – Spawn when max tension > 30 (up to 32 nodes)

### Drivers
- **VGA text** – 80×25, scroll
- **PS/2 keyboard** – Shift, caps lock, 128-byte ring buffer
- **Serial** – COM1

### Filesystem
- **In-RAM tree** – mkdir, touch, read, write, list
- **Persistence** – Serialize to reserved region every 30s, on shutdown; restore on boot

---

## Build & Run

### Requirements
- Rust (nightly)
- **C compiler** (gcc or clang) – for assembling `idt.S` (build script)
- GNU Make, xorriso, QEMU (optional)

On Windows, use WSL, MSYS2, or install a cross-toolchain (e.g. `x86_64-elf-gcc`).

### Build
```bash
make all
```

### Run
```bash
make run
```

---

## Project Layout

```
BoggersTheOS/
├── kernel/src/
│   ├── main.rs      # Kernel, scheduler, syscalls
│   ├── allocator.rs # Bump heap (64 KiB)
│   ├── vga.rs
│   ├── keyboard.rs  # Shift, caps, ring buffer
│   ├── fs.rs
│   ├── persist.rs   # Checkpoint/restore
│   ├── shell.rs
│   └── idt.S        # Timer + syscall stubs
└── README.md
```

---

## Honest Limitations

- **No paging** – All processes share kernel address space; no isolation
- **Bump allocator** – No free; heap grows until exhaustion
- **Persistence** – In-RAM only; lost on power cycle
- **No disk** – No persistent storage
- **Keyboard** – US QWERTY scancode set 1 only
- **Shell** – Minimal (help, ps, echo, spawn, ls, cat, touch, mkdir, shutdown)

---

## Current Capabilities

- Boots on real hardware / QEMU
- User-mode shell with help, ps, echo, spawn, ls, cat, touch, mkdir, shutdown
- Dynamic process creation (up to 32)
- Checkpoint/restore of fs (and graph count) every 30s and on shutdown
- VGA + serial output, keyboard input
- Strongest Node scheduler with emergence

---

## Remaining Work

1. **Restore paging** – Per-process page directories, CR3 switch
2. **Heap with free** – Linked-list or similar for long-running use
3. **Disk persistence** – Write checkpoint to disk
4. **More shell commands** – cd, pwd, rm
5. **Syscall validation** – Bounds-check user pointers

---

## Philosophy & Design Principles

- **Strongest Node drives everything** – Kernel is core; all else emerges
- **Secondary nodes = processes** – Process graph with activation, tension, neighbors
- **Emergence** – Nodes spawn when tension high
- **Tension resolution** – Bugs, inefficiency, bloat are tensions
- **Coherence rollback** – Every change triggers coherence check

---

## License

MIT (same as BoggersTheCIG).

---

*Built with the Strongest Node cognitive loop from BoggersTheCIG.*
