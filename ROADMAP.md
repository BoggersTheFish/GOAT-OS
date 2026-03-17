# TS-OS Full Development Roadmap

A 6-phase roadmap to evolve TS-OS from its current minimal state into a usable OS with memory protection, persistent storage, a proper shell, and basic ecosystem—prioritizing stability and usability over performance.

---

## Current State (as of 2025-03-17)

| Component     | Status                                                                  |
| ------------- | ----------------------------------------------------------------------- |
| Scheduler     | Strongest Node works (activation − tension, spread, decay, emergence)   |
| Process model | Ring 3, 32 nodes max, dynamic emergence                                 |
| Output        | **Serial only** (VGA framebuffer driver exists but does not display in QEMU) |
| Input         | PS/2 keyboard, 128-byte ring buffer                                     |
| Heap          | Bump allocator, **256 KiB**, no free — boot stable                      |
| Memory        | No paging; all processes share kernel address space                     |
| Storage       | In-RAM FS only; checkpoint to reserved region (lost on power cycle)     |
| Shell         | help, ps, echo, spawn, ls, cat, touch, mkdir, shutdown — no cd, pwd, rm |

---

## Phase 1: Stability Foundation — **Partially done**

**Main goals:** Fix boot crash, restore VGA, replace bump allocator with one that supports free. Get a stable, visible system.

| Task | Status |
|------|--------|
| Fix allocator (256 KiB or linked-list) | ✅ Done – 256 KiB bump, boot stable |
| Resolve ALLOC ERROR | ✅ Done |
| Restore VGA (Limine framebuffer) | ⚠️ Partial – driver implemented, display not visible in QEMU |
| Remove debug instrumentation | ✅ Done |

**Remaining:** VGA display in QEMU window. Framebuffer is received (e.g. 1280×800 at 0x1FFFF0001FA00000). Applying HHDM for addresses ≥ 4 GB causes PANIC; using address as-is yields no visible output. Needs further investigation (address mapping, pixel format, or QEMU config).

---

## Phase 2: Memory Protection — **Not started**

**Main goals:** Per-process address spaces, CR3 switch on schedule.

- Implement 4-level paging (identity-map kernel, user 0–2 MiB per process)
- Add `cr3` back to `ProcessNode`; create page directory on spawn
- Restore `paging::switch_cr3` in `do_schedule` and timer path
- Map user stack per process
- Syscall validation: bounds-check user pointers

**Dependencies:** Phase 1 (stable boot, working allocator)

---

## Phase 3: Persistent Storage — **Not started**

**Main goals:** Disk driver, block layer, FS on disk.

- IDE or AHCI driver for QEMU's virtio-blk or IDE disk
- Block layer: read/write sectors
- Extend in-RAM FS to read/write from disk
- Checkpoint to disk instead of reserved RAM region

**Dependencies:** Phase 1, Phase 2 (recommended)

---

## Phase 4: Usable Shell — **Not started**

**Main goals:** Navigate directories, delete files, pipes, basic scripting.

- `cd`, `pwd`, `rm`
- Pipes: `cmd1 | cmd2`
- Redirection: `cmd > file`, `cmd < file`
- Command history (up/down arrows)
- Simple line editor: backspace, left/right

**Dependencies:** Phase 3

---

## Phase 5: Multi-Process and Robustness — **Not started**

**Main goals:** Proper process lifecycle, signals, more syscalls.

- `fork` and `exec` (or `spawn` with path to binary)
- `wait`/`waitpid`
- Signals: SIGKILL, SIGTERM
- More syscalls: `open`/`close`/`read`/`write` with fd table, `dup2`, `chdir`, `getpid`, `getcwd`
- OOM handling: kill lowest-activation process when heap exhausted

**Dependencies:** Phases 2, 3, 4

---

## Phase 6: Ecosystem and Polish — **Not started**

**Main goals:** Installable software, basic utilities, networking.

- Package/install concept
- Core utilities: grep, find, sort, wc, head, tail
- Simple text editor
- Networking: virtio-net or e1000 driver, TCP/IP stack
- Boot time optimization, init script
- Documentation: man pages or `help <cmd>`

**Dependencies:** Phases 1–5

---

## Honest Assessment

| Phase | Estimated effort | Risk               |
| ----- | ---------------- | ------------------ |
| 1     | 1–2 weeks        | Low                |
| 2     | 2–4 weeks        | High (paging bugs) |
| 3     | 2–4 weeks        | High (hardware)    |
| 4     | 1–2 weeks        | Medium             |
| 5     | 3–5 weeks        | High               |
| 6     | 4–8 weeks        | High               |

**Total:** Roughly 4–6 months of focused work for a single developer to reach "usable hobby OS" level. This plan targets **CLI parity** and **stability**.
