# Contributing to BTFOS / GOAT-OS

Thanks for your interest. Contributions (code, docs, issues, feedback) are welcome.

## How to contribute

1. **Build and run**
   - Check dependencies: `./scripts/check_deps.sh` (or `scripts\check_deps.ps1` on Windows).
   - Build: `make`. Run: `make run` (QEMU).
   - Verify boot: `./scripts/verify_boot.sh` (or `scripts\verify_boot.ps1`). Must see "BTFOS Ready" on serial.

2. **Report issues**
   - Use [GitHub Issues](https://github.com/BoggersTheFish/GOAT-OS/issues).
   - For bugs: include OS, toolchain (nasm, gcc, QEMU versions), and steps to reproduce. If you get 404 or "no code" when cloning, say so and we can fix repo layout.
   - For ideas: use the "Feature request" template if available.

3. **Pull requests**
   - Open a PR against `master` (or `main`). Ensure CI passes (build + QEMU boot verification).
   - Keep changes focused; prefer small PRs. New features: extend the GOAT-TS logic (graph, cognition loop, ingestion) where it fits.

4. **Code**
   - Kernel: C (no external libs beyond what’s in tree). Preserve existing style and comments.
   - Config: presets in `include/btfos_config.h`; document new options in README or docs.

5. **Community**
   - Be respectful. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

Good first areas: more shell commands, benchmark presets, CI improvements, docs (HARDWARE, ROADMAP), or fixing build on new platforms.
