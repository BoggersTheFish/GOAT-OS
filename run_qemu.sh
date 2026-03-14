#!/bin/sh
# BTFOS - Run in QEMU. MIT License.
KERNEL=btfos.elf
[ -f "$KERNEL" ] || make
exec qemu-system-i386 -kernel "$KERNEL" -serial stdio -display none
