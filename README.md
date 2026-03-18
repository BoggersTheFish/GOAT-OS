# TS-OS – Strongest Node Operating System (Rust Implementation)

TS-OS is a bare-metal x86_64 microkernel that directly instantiates the **Strongest Node Framework** from [BoggersTheCIG](https://github.com/BoggersTheFish/BoggersTheCIG). The kernel is the Strongest Node; every other component exists only as secondary nodes that dynamically emerge, strengthen, spread activation, detect tension, decay, merge, or get pruned.

**This repo replaces the previous GOAT-OS C prototype.** It is the first full application of the TS framework to the OS domain.

---

## Current Status (TS-State Summary)

| Component | Status |
|-----------|--------|
| **Strongest Node** | Robust TS-OS (activation 100, tension 8) |
| **Paging** | 4-level paging, CR3 switch per process |
| **Heap** | `linked_list_allocator::LockedHeap` (demand-mapped via #PF) |
| **Keyboard** | Shift, caps lock, arrows, backspace, 128-byte buffer |
| **Timer / Preemption** | PIT IRQ0 re-entrancy persists; `timer_handler` storms and kernel double-faults (DF/HCF) |
| **Persistence** | RAM + disk checkpoint; restore from disk on boot |
| **Process graph** | Vec-based, up to 32 nodes, parent-child, wait |
| **Process isolation** | Per-process page tables (CR3), user VMAs + demand paging |
| **VGA** | Limine framebuffer (pixel) or 0xB8000+HHDM fallback; null checks; hex debug; "TS-OS Strongest Node online" |
| **Disk** | IDE PIO driver, 16 MB disk.img |

**Tensions resolved:** Allocator, VGA (visible in QEMU), paging, disk, shell (cd, pwd, rm, pipes, history), wait/kill

**Boot status:** Full boot: Limine, GDT/IDT/TSS, VGA, fs, persist restore, Strongest Node scheduler, shell. `make run` boots to interactive shell.

---

## Implemented Features

### Core
- **Boot with Limine** – x86_64 bare-metal (BIOS + UEFI)
- **GDT + TSS** – Kernel + user segments, TSS for kernel stack
- **Ring 3 user mode** – User CS/SS, iretq, syscall DPL=3
- **Kernel heap** – demand-mapped heap region in upper-half VA space
- **ELF loader** – demand-friendly PT_LOAD mapping (VMAs + file-backed pages only)
- **execve (in-place)** – loads ELF into fresh CR3/AddressSpace, installs user stack VMA, patches iret frame
- **Scheduler module** – Extracted to scheduler.rs with prune_dead_nodes stub

### Syscalls
| # | Name | Description |
|---|------|-------------|
| 1 | write | stdout → VGA first, then serial |
| 2 | yield | Yield to scheduler |
| 3 | spawn | Spawn process |
| 4 | read | stdin from keyboard first, then serial |
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
- **VGA** – Limine framebuffer, 8×8 font, 80×25 text grid, status bar (nodes/act/tension). Primary output.
- **PS/2 keyboard** – Shift, caps lock, arrows, backspace. Primary input.
- **Serial** – COM1 + early `log!()` macro (safe from interrupt context); use `make run-debug` for host-visible tracing

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

### Run (VGA-only, clean in-VM experience)
```bash
make run
```
Creates `disk.img` (16 MB) if missing. QEMU opens a **VGA window**—all interaction happens inside the OS. No serial; VGA is the primary output.

For serial output to the host terminal (debugging):
```bash
make run-debug
```

#### Debugging execve / user-mode bringup
- Use `make run-debug` and watch serial for:
  - `execve:` step-by-step trace (file read → new CR3 → load ELF → stack mapping → switch_cr3 → iret patch)
  - `PF:` page-fault classification (kernel-heap vs user VMA vs invalid) with CR2 + error bits
  - `timer:` context switch announcements when preemption switches processes

---

## How to Boot and Use

### Boot sequence (minimal kernel)
1. Limine loads kernel; requests Framebuffer + HHDM.
2. Kernel checks responses: if NULL → serial "LIMINE FB RESPONSE NULL" / "HHDM RESPONSE NULL" and fallback to 0xB8000+HHDM.
3. Hex debug via serial: FB ADDR, HHDM OFFSET, FB VIRT.
4. If fb_virt invalid (0 or < 0x1000) → "INVALID FB ADDR" + halt.
5. Use Limine framebuffer (pixel) if valid (≥640×400); else VGA text at 0xB8000+HHDM.
6. Clear screen, print "TS-OS Strongest Node online" on VGA.
7. Halt (shell not started until VGA confirmed).

### Interactive use (when shell enabled)
1. Run `make run`. A QEMU window opens.
2. At the **Limine menu**, select **TS-OS** and press **ENTER**.
3. The kernel boots and shows a welcome screen. **Click the QEMU window** to give it keyboard focus.
4. You will see `> `—type commands and press ENTER.

**First commands to try:**
| Command | What it does |
|---------|--------------|
| `help` | List all commands |
| `ps` | Show processes (Strongest Node graph) |
| `ls` | List files in current directory |
| `spawn` | Spawn a new process (node emerges) |
| `cat readme.txt` | Read the readme file |

**Important:** You must **click the QEMU window** before typing. The window must have focus to receive keyboard input.

---

## First Boot Experience

1. **Kernel boot** – "TS-OS Strongest Node online" (framebuffer cleared of Limine text).
2. **Welcome screen** – Strongest Node intro, "CLICK THIS WINDOW TO TYPE", and command hints.
3. **Shell prompt** – `> ` appears. Click the window and type.

| Command | Description |
|---------|--------------|
| `help` | Full command list; `help ls` for per-command help |
| `cd`, `pwd` | Change and print working directory |
| `ls`, `cat`, `touch`, `mkdir`, `rm` | File operations |
| `ls \| cat out.txt` | Pipes (e.g. ls output to file) |
| `cmd > file`, `cmd < file` | Redirection |
| `getpid`, `wait`, `kill PID` | Process control |
| `wc`, `head`, `tail` | Text utilities |

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

## Current Limitations

- **Keyboard** – US QWERTY scancode set 1 only; click the QEMU window to give it focus before typing
- **VGA** – Framebuffer via Limine; if the window stays black, try `make run-debug` and use serial
- **fork** – not implemented yet
- **execve** – implemented, but user program bringup is still under active debug
- **Timer / preemption** – PIT IRQ0 handling currently fails under load: serial prints repeated `timer_handler entered` and then `DF` followed by `HCF` (double-fault path). This makes scheduler preemption unstable.
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

---

## TS-State Summary (This Step)

| Metric | Before | After |
|--------|--------|-------|
| Boot | Stuck at Limine menu; kernel never starts | _start → kmain → serial_init → "K1 - Kernel entry" |
| Entry point | ENTRY(kmain) | ENTRY(_start); _start in .text._start calls kmain |
| Kernel | Full kmain (allocator, VGA, etc.) | Reaches scheduler/timer hot path, but crashes on PIT timer re-entrancy |
| limine.conf | 1-space indent | 4-space indent (proper format) |

**Boot behavior:** Diagnostic iteration. _start (assembly) → kmain → serial_init → serial_write("K1 - Kernel entry\r\n") → boot init → enters PIT timer ISR, repeatedly logs `timer_handler entered`, then triggers `DF`/`HCF` due to persistent timer re-entrancy.

**Exact test commands (WSL/MSYS2):**
```bash
cd BoggersTheOS
make clean && make all && make run-debug
# 1. At Limine: select TS-OS, press ENTER
# 2. If kernel entry works: serial shows "K1 - Kernel entry"
# 3. If still stuck at menu: report back; Limine may not be loading/jumping to kernel
```

---

*Built with the Strongest Node cognitive loop from BoggersTheCIG.*
