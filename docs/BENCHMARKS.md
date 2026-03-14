# BTFOS Benchmarks

Performance is **not** claimed (e.g. no "20–30% faster" without data). This doc describes how to measure and compare.

## Benchmark mode

Build with the **benchmark** preset to run a fixed number of cognition-loop ticks and print machine-readable results.

1. In `include/btfos_config.h`, set:
   ```c
   #define BTFOS_PRESET BTFOS_BENCHMARK
   ```
2. Rebuild: `make clean && make`
3. Run: `make run` (or `qemu-system-i386 -kernel btfos.elf -serial stdio -display none`)
4. Serial output will include:
   - `{"benchmark_ticks":10000,"benchmark_done":1}`
   - "BTFOS benchmark done."

Measure **wall-clock time** for the run (e.g. `time make run`). Then:

- **Ticks per second** ≈ `benchmark_ticks / elapsed_seconds`
- Compare presets (e.g. **BTFOS_LOW_POWER** vs **BTFOS_NORMAL**) on the same host to see graph-overhead impact.

## Low-power preset

For low-power or slow CPUs (e.g. Pentium Silver), use **BTFOS_LOW_POWER**:

- Fewer graph nodes/edges (smaller caps).
- No forces, reflection, or curiosity in the hot path.
- Single shard.

This reduces graph overhead; benchmark both and compare ticks/sec and responsiveness.

## CI

The GitHub Action runs a short boot verification (expects "BTFOS Ready"). It does **not** run the full benchmark. Run benchmarks locally and, if you want, publish results in an Issue or the wiki.
