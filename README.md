# TS-OS – Strongest Node Operating System (Rust Implementation)

TS-OS is a bare-metal x86_64 microkernel that directly instantiates the **Strongest Node Framework** from [BoggersTheCIG](https://github.com/BoggersTheFish/BoggersTheCIG). The kernel is the Strongest Node; every other component exists only as secondary nodes that dynamically emerge, strengthen, spread activation, detect tension, decay, merge, or get pruned.

**This repo replaces the previous GOAT-OS C prototype.** It is the first full application of the TS framework to the OS domain.

---

## Current Status (TS-State Summary)

| Component | Status |
|-----------|--------|
| **Strongest Node** | Robust TS-OS (activation 100, tension 8) |
| **Paging** | 4-level, identity map kernel, user 0–2 MiB (U=1) |
| **Heap** | Linked-list with coalescing + defrag (frag > 30%) |
| **Keyboard** | Shift, caps lock, 128-byte circular buffer |
| **Persistence** | Checkpoint graph+fs every 30s, restore on boot |
| **Process graph** | Vec-based, up to 32 nodes, dynamic emergence |
| **Process isolation** | Per-process page directory, CR3 switch on schedule |

### Node Table (Example)

| Node | id | activation | tension | cr3 |
|------|-----|------------|---------|-----|
| 0 | 0 | 100 | 0 | 0 (shared) |
| 1 | 1 | 80 | 10 | 0 |
| 2 | 2 | 60 | 5 | 0 |
| 5 | 5 | 50 | 0 | per-process |

**Tensions resolved:** No paging, no coalescing, basic keyboard, no persistence, fixed 8 nodes, no isolation  
**Tensions remaining:** Persistence is in-RAM (lost on power cycle), no disk, shell still minimal  
**Coherence delta:** +6 (full iteration)

---

## Implemented Features

### Core
- **Boot with Limine** – x86_64 bare-metal (BIOS + UEFI)
- **4-level paging** – Identity map kernel, user 0–2 MiB (U=1), per-process page dirs
- **Linked-list heap** – 128 KiB, coalescing on free, defrag when frag > 30%
- **GDT + TSS** – Kernel + user segments, TSS for kernel stack
- **Ring 3 user mode** – User CS/SS, iretq, syscall DPL=3

### Syscalls
| # | Name | Description |
|---|------|-------------|
| 1 | write | stdout → VGA + serial |
| 2 | yield | Yield to scheduler |
| 3 | spawn | Spawn process (own page dir) |
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

### Process Isolation
- **Per-process page directory** – Spawned processes get own CR3
- **User stack** – Mapped at 0x1000–0x2000 per process

---

## Build & Run

### Requirements
- Rust (nightly)
- GNU Make, xorriso, QEMU (optional)

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
│   ├── allocator.rs # Heap + coalescing + defrag
│   ├── paging.rs    # 4-level paging, per-process CR3
│   ├── vga.rs
│   ├── keyboard.rs  # Shift, caps, ring buffer
│   ├── fs.rs
│   ├── persist.rs   # Checkpoint/restore
│   ├── shell.rs
│   └── idt.S
└── README.md
```

---

## Honest Limitations

- **Persistence** – In-RAM only; lost on power cycle
- **No disk** – No persistent storage
- **Keyboard** – US QWERTY scancode set 1 only
- **No coalescing** – Defrag merges adjacent free blocks only; no moving allocated blocks
- **Process isolation** – Per-process address space; kernel still shared
- **Shell** – Minimal

---

## Current Capabilities

- Boots on real hardware / QEMU
- User-mode shell with help, ps, echo, spawn, ls, cat, touch, mkdir, shutdown
- Dynamic process creation (up to 32)
- Per-process page tables for spawned processes
- Checkpoint/restore of fs (and graph count) every 30s and on shutdown
- VGA + serial output, keyboard input
- Strongest Node scheduler with emergence

---

## Remaining Work

1. **Disk persistence** – Write checkpoint to disk
2. **Full heap defrag** – Move allocated blocks
3. **More shell commands** – cd, pwd, rm
4. **Syscall validation** – Bounds-check user pointers

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
