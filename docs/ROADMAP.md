# BTFOS / GOAT-OS Roadmap

## Current scope (minimal, not production-ready)

- **Kernel**: Graph-driven scheduler (GOAT-TS style), activation/decay, forces, reflection, curiosity; process table; stub syscalls (fork, exec, read, write, open, close, exit, getpid, yield, gettime).
- **FS**: In-memory triple store (ingest as text); mkdir/stat; no persistent block device.
- **Drivers**: VGA text and serial only (stubs for plugins).
- **No networking**, no real device drivers, no 64-bit.

## Toward production readiness

| Area | Status | Next steps |
|------|--------|------------|
| **Syscalls** | Stubs + graph ingestion | Real fork/exec (copy process table, load ELF); blocking I/O |
| **FS** | In-memory triples | Persistent FS (ext2 stub or simple block); file content in triples |
| **Memory** | Graph-based page states | Real paging, swap, DORMANT/DEEP (compress/swap) |
| **Drivers** | Stub list (VGA, serial) | Loadable modules from disk; ATA/NVMe stub |
| **Networking** | None | Protocol stack stub; ingest packets as graph events |
| **64-bit** | No | Long mode switch; 64-bit kernel build |
| **SMP** | Logical shards only | Real AP bring-up; per-CPU run queues |
| **Security** | None | Rings, syscall gates, capability model |

## Community

Issues and PRs welcome. See [CONTRIBUTING.md](../CONTRIBUTING.md). "Good first issue" and "help wanted" labels will be used as the project grows.
