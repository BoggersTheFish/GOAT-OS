#!/bin/sh
# BTFOS - Verify build/run dependencies. Exit 0 if all present.
missing=
command -v nasm >/dev/null 2>&1 || missing="$missing nasm"
command -v gcc >/dev/null 2>&1 || missing="$missing gcc"
gcc -m32 --version >/dev/null 2>&1 || missing="$missing gcc-m32"
command -v qemu-system-i386 >/dev/null 2>&1 || command -v qemu-system-x86 >/dev/null 2>&1 || missing="$missing qemu-system-i386"
if [ -n "$missing" ]; then
    echo "Missing:$missing"
    echo "Install: nasm, gcc (32-bit), qemu-system-x86"
    exit 1
fi
echo "OK: nasm gcc qemu"
exit 0
