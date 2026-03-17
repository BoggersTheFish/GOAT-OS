# TS-OS – Strongest Node Operating System (Rust Implementation)

TS-OS is a bare-metal x86_64 microkernel that directly instantiates the **Strongest Node Framework** from [BoggersTheCIG](https://github.com/BoggersTheFish/BoggersTheCIG). The kernel is the Strongest Node; every other component exists only as secondary nodes that dynamically emerge, strengthen, spread activation, detect tension, decay, merge, or get pruned.

**This repo replaces the previous GOAT-OS C prototype.** It is the first full application of the TS framework to the OS domain.

---

## Current Status (TS-State Summary)

| Component | Status |
|-----------|--------|
| **Strongest Node** | Full usable TS-OS (kernel + scheduler, activation 100, tension 0) |
| **Secondary nodes** | 5 static + dynamic emergence when tension > 30 |
| **User mode** | Ring 3 via GDT user segments, iretq, syscall DPL=3 |
| **Heap** | Linked-list allocator with free (128 KiB) |
| **Syscalls** | write, yield, spawn, read, exit, ls, cat, ps, touch, mkdir, write_f |
| **Drivers** | VGA text (0xB8000), PS/2 keyboard, serial (COM1) |
| **Filesystem** | In-RAM tree (nodes as files/directories) |
| **Shell** | User-mode shell (help, ps, echo, spawn, ls, cat, touch, mkdir, exit) |

### Node Table (Initial)

| Node | id | activation | tension | role |
|------|-----|------------|---------|------|
| 0 | 0 | 100 | 0 | Shell |
| 1 | 1 | 80 | 10 | Worker |
| 2 | 2 | 60 | 5 | Worker |
| 3 | 3 | 40 | 20 | Worker |
| 4 | 4 | 80 | 0 | Worker |

**Tensions resolved:** Ring 3, heap free, syscalls, VGA, keyboard, filesystem, shell  
**Tensions remaining:** No paging (identity map; ring 3 may fault on some hardware), no coalescing in allocator, keyboard scancode set 1 only  
**Coherence delta:** +7 (full iteration complete)

---

## Implemented Features

### Core
- **Boot with Limine** – x86_64 bare-metal boot (BIOS + UEFI)
- **Linked-list heap** – 128 KiB, GlobalAlloc with alloc + dealloc, first-fit, block splitting
- **GDT + TSS** – Kernel + user code/data segments, TSS with kernel stack for interrupts
- **Ring 3 user mode** – User CS (0x1b), user SS (0x23), iretq to user code
- **Syscall interface (INT 0x80)** – DPL=3 so user can invoke

### Syscalls
| # | Name | Description |
|---|------|-------------|
| 1 | write | Write to fd 1 (stdout → VGA + serial) |
| 2 | yield | Yield to scheduler |
| 3 | spawn | Spawn new process node |
| 4 | read | Read from fd 0 (stdin → keyboard) |
| 5 | exit | Exit process |
| 6 | ls | List directory |
| 7 | cat | Read file |
| 8 | ps | List processes |
| 9 | touch | Create file |
| 10 | mkdir | Create directory |
| 11 | write_f | Write to file |

### Scheduler
- **Strongest Node** – Select by `activation - tension`
- **Spreading activation** – Running node spreads +10 to neighbors
- **Decay** – Inactive nodes lose 2 activation per tick
- **Tension bump** – Preempted node gains +1 tension
- **Dynamic emergence** – When max tension > 30, spawn new node (if room)

### Drivers
- **VGA text** – 80×25, 0xB8000, scroll on newline
- **PS/2 keyboard** – Ports 0x60/0x64, scancode set 1 → ASCII
- **Serial** – COM1 (0x3F8) for debug

### Filesystem
- **In-RAM tree** – Nodes as files/directories
- **Operations** – mkdir, touch, read_file, write_file, list_dir
- **Paths** – Unix-style (/path/to/file)

### Shell (User Mode)
- **Commands:** help, ps, echo, spawn, ls, cat, touch, mkdir, exit
- **Echo to file:** `echo "text" > path`
- **Runs as node 0** – First scheduled process

---

## Build & Run

### Requirements
- **Rust** (nightly): `rustup default nightly`
- **GNU Make** (MSYS2, WSL, or MinGW on Windows)
- **xorriso** (for ISO generation)
- **QEMU** (optional)

### Build
```bash
make all
```
Produces `ts-os-x86_64.iso`.

### Run in QEMU
```bash
make run
```
VGA appears in QEMU window; serial in terminal (`-serial stdio`).

### UEFI
```bash
make run-uefi
```

---

## Project Layout

```
BoggersTheOS/
├── .cursorrules.txt
├── GNUmakefile
├── limine.conf
├── kernel/
│   ├── Cargo.toml
│   ├── build.rs
│   ├── linker-x86_64.ld
│   └── src/
│       ├── main.rs      # Kernel entry, scheduler, syscalls
│       ├── allocator.rs # Linked-list heap
│       ├── vga.rs       # VGA text driver
│       ├── keyboard.rs # PS/2 keyboard
│       ├── fs.rs        # In-RAM filesystem
│       ├── shell.rs     # User-mode shell
│       └── idt.S        # Timer + syscall stubs
└── README.md
```

---

## Honest Limitations

- **No paging** – Identity mapping from bootloader; ring 3 may fault if pages lack user bit
- **No coalescing** – Free blocks not merged; fragmentation possible
- **Fixed node cap** – MAX_NODES=8; graph is static array
- **Keyboard** – US QWERTY scancode set 1 only; no shift/caps
- **Filesystem** – In-RAM only; lost on reboot
- **No disk** – No persistent storage
- **Single address space** – No process isolation

---

## Roadmap

1. **Paging** – User-accessible pages for reliable ring 3
2. **Heap coalescing** – Merge adjacent free blocks
3. **Dynamic graph** – Heap-allocated ProcessGraph, grow beyond 8 nodes
4. **Process isolation** – Per-process address spaces

---

## License

MIT (same as BoggersTheCIG).

---

*Built with the Strongest Node cognitive loop from BoggersTheCIG.*
