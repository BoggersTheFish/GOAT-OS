# TS-OS – Strongest Node Operating System (Rust Implementation)

TS-OS is a bare-metal x86_64 microkernel that directly instantiates the **Strongest Node Framework** from [BoggersTheCIG](https://github.com/BoggersTheFish/BoggersTheCIG). The kernel is the Strongest Node; every other component exists only as secondary nodes that dynamically emerge, strengthen, spread activation, detect tension, decay, merge, or get pruned. The architecture follows the full cognitive loop: kernel = core_engine (structural search, pattern discovery, constraint resolution); emergence = triple extraction → graph nodes (processes); spreading activation = scheduler decisions; tension detection = bugs, inefficiency, contradiction, bloat; memory decay = prune inactive/low-degree nodes; convergence = self-organization; self-improvement = coherence check with rollback on failure.

**This repo replaces the previous GOAT-OS C prototype.** It is the first full application of the TS framework from https://github.com/BoggersTheFish/BoggersTheCIG to the OS domain.

---

## Current Status (TS-State Summary)

| Component | Status |
|-----------|--------|
| **Strongest Node** | User-mode foundation + syscall interface (activation 100, tension 0) |
| **Secondary nodes** | 5 process nodes in static graph |

### Node Table

| Node | id | activation | tension | neighbors |
|------|-----|------------|---------|-----------|
| 0 | 0 | 100 | 0 | 1, 3 |
| 1 | 1 | 80 | 10 | 0, 2 |
| 2 | 2 | 60 | 5 | 1, 3 |
| 3 | 3 | 40 | 20 | 2, 0 |
| 4 | 4 | 80 | 0 | 0 |

**Tensions resolved:** No syscall interface  
**Tensions remaining:** No actual ring 3 (nodes still ring 0), no dynamic process creation, no filesystem, serial-only, no shell  
**Coherence delta:** +1 (GDT user segments, TSS, INT 0x80 syscalls)

---

## Implemented Features

- **Boot with Limine** – x86_64 bare-metal boot via Limine bootloader (BIOS + UEFI)
- **Bump heap allocator** – 64 KiB static backing, GlobalAlloc, atomic bump pointer, no free
- **GDT user segments + TSS** – User code/data segments (ring 3), TSS with kernel stack for interrupts
- **Syscall interface (INT 0x80)** – sys_write (1), sys_yield (2), sys_spawn (3, stub)
- **Node entries use syscalls** – do_sys_write, do_sys_yield via int 0x80
- **PIT timer interrupt** – Real ~10ms tick (11932 divisor), PIC remapped, IRQ0 → vector 32
- **Preemptive scheduler** – Timer fires, saves full context, runs decay/spread/select, switches via iretq
- **GDT + IDT** – x86_64 crate for GDT (kernel + user segments, TSS), IDT with timer + syscall stubs
- **Static ProcessGraph** – In-RAM process graph with `[ProcessNode; 8]`
- **Per-node 4 KiB stacks** – Each node has `stack: [u8; 4096]`, `saved_rip`, `saved_rsp`
- **Real context switch** – Assembly stub saves GPRs, timer_handler/syscall_handler returns new frame ptr, iretq restores
- **Spreading activation scheduler** – Select strongest node by `activation - tension`; running node spreads +10 to neighbors
- **Decay** – Inactive nodes lose 2 activation per tick
- **Switch only when different** – Context switch only when the newly selected strongest node differs from the previous
- **Serial output** – COM1 (0x3F8) for all output; QEMU `-serial stdio` shows output in terminal
- **Node entry functions** – Node 0 stats, Node 1 "alive", Node 4 "working"; use sys_write + sys_yield

---

## Build & Run

### Requirements

- **Rust** (nightly): `rustup default nightly`
- **GNU Make** (MSYS2, WSL, or MinGW on Windows)
- **xorriso** (for ISO generation)
- **QEMU** (optional, for testing)

### Build

```bash
make all
```

Produces `ts-os-x86_64.iso`.

### Run in QEMU

```bash
make run
```

Serial output appears in the terminal (QEMU uses `-serial stdio` by default).

### UEFI Run

```bash
make run-uefi
```

### Windows Fallback

If `make` is unavailable, build the kernel only:

```powershell
cd kernel
cargo build --target x86_64-unknown-none
```

Copy `target/x86_64-unknown-none/debug/ts-os-kernel` to `kernel/kernel` and use xorriso/Limine manually for ISO creation.

---

## Project Layout

```
BoggersTheOS/
├── .cursorrules.txt      # TS-OS Builder rules (Strongest Node loop)
├── .gitignore
├── GNUmakefile           # Root build (Limine + kernel → ISO)
├── limine.conf           # Bootloader config
├── LICENSE               # MIT
├── README.md
├── build.ps1             # Windows kernel-only build helper
└── kernel/
    ├── Cargo.toml        # ts-os-kernel, limine 0.5, x86_64 0.14
    ├── build.rs
    ├── GNUmakefile
    ├── linker-x86_64.ld
    ├── rust-toolchain.toml  # nightly, x86_64-unknown-none
    └── src/
        ├── main.rs       # ~400 LOC
        └── idt.S         # Timer interrupt stub (minimal asm)
```

---

## .cursorrules Summary (Iron-Clad Rules)

- **Essential features ONLY** – Prune aggressively; nothing beyond boot, one user process, self-stabilization
- **Microkernel architecture** – Kernel stays tiny (< 10k LOC target)
- **Written 100% in Rust** – No C, no assembly except minimal boot
- **Limine bootloader** – Real hardware boot capability
- **No POSIX, no full libc** – No unnecessary drivers in v1
- **Scheduler = Strongest Node** – Pure weighted spreading activation, no traditional priority queues
- **File system = in-RAM node graph** – No disk yet
- **After every change** – State Strongest Node, list secondary nodes, detect tensions, generate hypotheses, choose simplest, implement, coherence check, rollback if needed

---

## Philosophy & Design Principles

- **Strongest Node drives everything** – The kernel is the core; all else emerges as secondary nodes
- **Secondary nodes = processes** – Process graph with activation, tension, neighbors
- **Emergence** – Nodes are added at boot; future: triple extraction from workload
- **Tension resolution** – Bugs, inefficiency, contradiction, bloat are tensions; detected and resolved
- **Coherence rollback** – Every change triggers coherence check; if coherence drops >10% or tests fail → automatic rollback + backup
- **Never design the whole OS at once** – Architecture emerges node-by-node

---

## Roadmap / Next Hypotheses

After push, the next Strongest Node candidates:

1. **Actual ring 3** – iretq to user CS/SS (requires user-accessible page; paging changes)
2. **Dynamic process emergence** – sys_spawn creates nodes at runtime using heap
3. **In-RAM file system** – Hierarchical node graph as files (no disk, pure RAM)
4. **Process emergence from workload** – Triple extraction: spawn nodes from detected patterns

---

## Current Limitations

This is still a minimal research kernel, not a complete usable OS. Nodes run in ring 0 (kernel mode); GDT/TSS and syscall interface are in place for future ring 3. No shell, no filesystem, no keyboard/VGA. Serial-only output. Process graph is fixed at boot. sys_spawn is a stub. The heap is bump-only (no free). Architecture emerges node-by-node per .cursorrules.

---

## License

MIT (same as BoggersTheCIG).

---

*Built continuously with the exact same cognitive loop as BoggersTheCIG.*
