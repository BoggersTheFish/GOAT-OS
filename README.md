# TS-OS – Strongest Node Operating System (Rust Implementation)

TS-OS is a bare-metal x86_64 microkernel that directly instantiates the **Strongest Node Framework** from [BoggersTheCIG](https://github.com/BoggersTheFish/BoggersTheCIG). The kernel is the Strongest Node; every other component exists only as secondary nodes that dynamically emerge, strengthen, spread activation, detect tension, decay, merge, or get pruned.

**This repo replaces the previous GOAT-OS C prototype.** It is the first full application of the TS framework to the OS domain.

---

## Current Status (TS-State Summary)

| Component | Status |
|-----------|--------|
| **Strongest Node** | Robust TS-OS (activation 100, tension 8) |
| **Paging** | 4-level paging, CR3 switch per process |
| **Heap** | Bump allocator (256 KiB, no free) |
| **Keyboard** | Shift, caps lock, arrows, backspace, 128-byte buffer |
| **Persistence** | RAM + disk checkpoint; restore from disk on boot |
| **Process graph** | Vec-based, up to 32 nodes, parent-child, wait |
| **Process isolation** | Per-process page tables, syscall validation |
| **VGA** | Limine framebuffer, HHDM mapping |
| **Disk** | IDE PIO driver, 16 MB disk.img |

**Tensions resolved:** Allocator, VGA, paging, disk, shell (cd, pwd, rm, pipes, history), wait/kill

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
| 4 | read | stdin from keyboard/serial |
| 5 | exit | Exit process |
| 6 | ls | List directory |
| 7 | cat | Read file |
| 8 | ps | List processes |
| 9 | touch | Create file |
| 10 | mkdir | Create directory |
| 11 | write_f | Write to file |
| 12 | shutdown | Checkpoint and halt |
| 13 | clear | Clear VGA screen |
| 14 | poll_key | Non-blocking key check |
| 15 | rm | Remove file or empty dir |
| 16 | getpid | Get process ID |
| 17 | chdir | Change working directory |
| 18 | getcwd | Get working directory |
| 19 | wait | Wait for child exit |
| 20 | kill | Send SIGKILL to process |

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
Creates `disk.img` (16 MB) if missing. QEMU boots with CD and IDE disk.

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
- `help` – Full command list; `help ls` for per-command help
- `cd`, `pwd` – Change and print working directory
- `ls`, `cat`, `touch`, `mkdir`, `rm` – File operations
- `ls \| cat out.txt` – Pipes (e.g. ls output to file)
- `cmd > file`, `cmd < file` – Redirection
- `getpid`, `wait`, `kill PID` – Process control
- `wc`, `head`, `tail` – Text utilities

---

## Project Layout

```
BoggersTheOS/
├── kernel/src/
│   ├── main.rs      # Kernel, scheduler, syscalls, paging
│   ├── allocator.rs # Bump heap (256 KiB)
│   ├── paging.rs    # 4-level paging, CR3 switch
│   ├── disk.rs      # IDE PIO driver
│   ├── vga.rs       # VGA text driver
│   ├── keyboard.rs  # PS/2, arrows, backspace
│   ├── fs.rs        # In-RAM filesystem
│   ├── persist.rs   # Checkpoint/restore (RAM + disk)
│   └── shell.rs     # Shell with cd, pwd, rm, pipes, history
├── disk.img         # 16 MB IDE disk (created by make)
└── README.md
```

---

## Honest Limitations

- **Keyboard** – US QWERTY scancode set 1 only
- **No fork/exec** – Spawn only; no ELF loader
- **No networking** – No TCP/IP stack
- **No fd table** – open/close/read/write via path, not fd

---

## Current Capabilities

- Boots on real hardware / QEMU with disk
- Per-process paging, syscall validation
- Bump allocator (256 KiB)
- VGA + serial output, keyboard + serial input
- Disk persistence (checkpoint to disk.img)
- Shell: cd, pwd, rm, pipes, redirection, history, getpid, wait, kill
- Utilities: wc, head, tail; help &lt;cmd&gt;
- Parent-child process tracking, wait for child exit

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
