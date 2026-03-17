# TS-OS – Strongest Node Operating System (Rust Implementation)

TS-OS is a bare-metal x86_64 microkernel that directly instantiates the **Strongest Node Framework** from [BoggersTheCIG](https://github.com/BoggersTheFish/BoggersTheCIG). The kernel is the Strongest Node; every other component exists only as secondary nodes that dynamically emerge, strengthen, spread activation, detect tension, decay, merge, or get pruned.

**This repo replaces the previous GOAT-OS C prototype.** It is the first full application of the TS framework to the OS domain.

---

## Current Status (TS-State Summary)

| Component | Status |
|-----------|--------|
| **Strongest Node** | Robust TS-OS (activation 100, tension 8) |
| **Paging** | **Removed** – identity map only; no per-process isolation |
| **Heap** | Bump allocator (256 KiB, no free) |
| **Keyboard** | Shift, caps lock, 128-byte circular buffer |
| **Persistence** | In-RAM checkpoint (fs + graph count); restore on boot |
| **Process graph** | Vec-based, up to 32 nodes, dynamic emergence |
| **Process isolation** | **None** – all processes share kernel address space |
| **VGA** | Framebuffer driver implemented; **serial-only output in practice** (QEMU window stays black) |

### Node Table (Example)

| Node | id | activation | tension |
|------|-----|------------|---------|
| 0 | 0 | 100 | 0 |
| 1 | 1 | 80 | 10 |
| 2 | 2 | 60 | 5 |
| 5 | 5 | 50 | 0 |

**Tensions resolved:** Allocator size, basic keyboard, fixed 32 nodes  
**Tensions remaining:** VGA not visible in QEMU, no process isolation, persistence in-RAM only, no disk, shell minimal  
**Coherence delta:** +4 (post-cleanup iteration)

---

## Implemented Features

### Core
- **Boot with Limine** – x86_64 bare-metal (BIOS + UEFI)
- **GDT + TSS** – Kernel + user segments, TSS for kernel stack
- **Ring 3 user mode** – User CS/SS, iretq, syscall DPL=3
- **Bump heap** – 256 KiB, no free (sufficient for current use)

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
| 13 | clear | Clear VGA screen |
| 14 | poll_key | Non-blocking key check (1 if available, 0 else) |

### Scheduler
- **Strongest Node** – activation − tension
- **Spreading activation** – +10 to neighbors
- **Decay** – −2 activation per tick
- **Tension bump** – +1 on preempt
- **Dynamic emergence** – Spawn when max tension > 30 (up to 32 nodes)

### Drivers
- **VGA** – Limine framebuffer request, 8×8 font, 80×25 text grid (code present; display not visible in QEMU—use serial)
- **PS/2 keyboard** – Shift, caps lock, 128-byte ring buffer
- **Serial** – COM1 (primary output)

### Filesystem
- **In-RAM tree** – mkdir, touch, read, write, list
- **Persistence** – Serialize to reserved region every 30s, on shutdown; restore on boot

---

## Build & Run

### Requirements
- Rust (nightly)
- GNU Make, xorriso, QEMU (optional)

On Windows, use WSL or MSYS2.

### Build
```bash
make all
```

### Run
```bash
make run
```

---

## First Boot Experience

When you boot TS-OS for the first time, **in the terminal** (where you ran `make run`) you will see:

1. **Kernel messages** – Serial init, framebuffer info (FB: 0x... -> 0x... 1280x800), process graph setup.
2. **Welcome screen** – A centered banner (via serial):
   ```
     ==========================================
               T S - O S
     ==========================================

     This is a living operating system powered by the
     Strongest Node Framework from BoggersTheCIG.

     Basic commands:
       help   - show full command list
       ps     - list processes (nodes)
       spawn  - spawn new process
       ...
     Type 'help' for full command list.
     Nodes emerge automatically based on system tension.

     Press any key to continue, or wait 4 seconds...
   ```
3. **Wait or skip** – The welcome stays for ~4 seconds, or press any key to continue.
4. **Shell prompt** – After the welcome, you get a normal `> ` prompt. Type commands in the terminal.

**Useful commands:**
- `help` – Full command list with descriptions
- `about` or `welcome` – Strongest Node philosophy and current status
- `ps` – List processes (nodes)
- `spawn` – Create a new process

**Booting:** At the Limine menu, press **ENTER** to boot. **Output is serial-only**—the QEMU window stays black. Use the terminal for the shell. The framebuffer is requested and receives a 1280×800 buffer, but writes do not appear in the QEMU display (address mapping issue).

---

## Project Layout

```
BoggersTheOS/
├── kernel/src/
│   ├── main.rs      # Kernel, scheduler, syscalls
│   ├── allocator.rs # Bump heap (256 KiB)
│   ├── vga.rs       # VGA text driver (Limine framebuffer)
│   ├── keyboard.rs  # PS/2, shift, caps, ring buffer
│   ├── fs.rs        # In-RAM filesystem
│   ├── persist.rs   # Checkpoint/restore
│   └── shell.rs     # Welcome screen, help, about, commands
└── README.md
```

---

## Honest Limitations

- **Serial-only output** – VGA framebuffer driver exists but display does not appear in QEMU window; use terminal for shell
- **No paging** – All processes share kernel address space; no isolation
- **Bump allocator** – No free; heap grows until exhaustion
- **Persistence** – In-RAM only; lost on power cycle
- **No disk** – No persistent storage
- **Keyboard** – US QWERTY scancode set 1 only
- **Shell** – help, about, ps, echo, spawn, ls, cat, touch, mkdir, shutdown

---

## Current Capabilities

- Boots on real hardware / QEMU
- User-mode shell with welcome screen, help, about, ps, echo, spawn, ls, cat, touch, mkdir, shutdown
- Dynamic process creation (up to 32)
- Checkpoint/restore of fs (and graph count) every 30s and on shutdown
- Serial output, keyboard input
- Strongest Node scheduler with emergence

---

## Remaining Work (per Roadmap)

### Phase 1 (Stability Foundation) – Partially done
- [x] Fix allocator: 256 KiB bump heap (boot no longer crashes)
- [x] Restore VGA: Limine framebuffer request, 8×8 font, 80×25 text grid
- [ ] **VGA display in QEMU** – Framebuffer address mapping (HHDM for >4GB addresses) causes PANIC; needs investigation
- [x] Remove debug instrumentation

### Phase 2+ (Future)
- **Restore paging** – Per-process page directories, CR3 switch
- **Heap with free** – Linked-list or similar for long-running use
- **Disk persistence** – Write checkpoint to disk
- **More shell commands** – cd, pwd, rm
- **Syscall validation** – Bounds-check user pointers

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
