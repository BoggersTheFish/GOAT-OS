#!/bin/bash
# TS-OS smoke test: boot in QEMU, verify shell prompt and basic commands.
# Requires: make, QEMU, expect (optional)
# Usage: from repo root, run: make test  OR  ./scripts/smoke_test.sh

set -e
cd "$(dirname "$0")/.."

echo "=== TS-OS Smoke Test ==="

# Build
echo "Building..."
make all 2>/dev/null || {
    echo "make all failed; trying cargo directly..."
    (cd kernel && RUSTFLAGS="-C relocation-model=static" cargo build --target x86_64-unknown-none)
    cp kernel/target/x86_64-unknown-none/debug/ts-os-kernel kernel/kernel 2>/dev/null || true
}

# Ensure disk exists
[ -f disk.img ] || {
    dd if=/dev/zero of=disk.img bs=1M count=16 2>/dev/null || \
    fsutil file createnew disk.img 16777216 2>/dev/null || \
    { echo "Create disk.img manually (16 MB)"; exit 1; }
}

# Run QEMU with serial to file, send commands, check output
OUT=$(mktemp)
trap "rm -f $OUT" EXIT

# Send Enter to get past Limine menu, wait for boot, then send commands
(
    sleep 2
    printf '\r'          # Select TS-OS at Limine menu
    sleep 10             # Wait for kernel boot + shell
    printf 'ls\r'
    sleep 2
    printf 'ps\r'
    sleep 2
) | (timeout 25 qemu-system-x86_64 -M q35 -cdrom ts-os-x86_64.iso -boot d \
    -drive file=disk.img,format=raw,if=ide -m 2G -serial stdio -display none 2>/dev/null || true) > "$OUT" 2>&1

# Check for expected output
if grep -q ">" "$OUT" && grep -q "pid" "$OUT"; then
    echo "PASS: Shell prompt and ps output detected"
else
    echo "FAIL: Expected shell output not found"
    echo "Last 50 lines of output:"
    tail -50 "$OUT"
    exit 1
fi

echo "=== Smoke test passed ==="
