#!/bin/sh
# BTFOS - Build, run in QEMU, assert "BTFOS Ready" in serial output.
set -e
cd "$(dirname "$0")/.."
./scripts/check_deps.sh
make clean
make
timeout 8 make run 2>&1 | tee serial.log
grep -q "BTFOS Ready" serial.log || { echo "Boot verification failed."; cat serial.log; exit 1; }
echo "BTFOS boot verified."
