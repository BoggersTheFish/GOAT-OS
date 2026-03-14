# BTFOS build script (PowerShell). Requires: nasm, i686-elf-gcc or gcc -m32, ld.
# On Windows: install NASM, MinGW-w64 (with 32-bit), and add to PATH. Or use WSL: make
$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$bootObj = Join-Path $root "boot\boot.o"
$kernelObj = Join-Path $root "kernel\kernel.o"
$target = Join-Path $root "btfos.elf"

if (-not (Get-Command nasm -ErrorAction SilentlyContinue)) {
    Write-Host "nasm not found. Install NASM and add to PATH."
    exit 1
}
$gcc = Get-Command gcc -ErrorAction SilentlyContinue
if (-not $gcc) {
    Write-Host "gcc not found. Install MinGW-w64 or use WSL and run: make"
    exit 1
}

& nasm -f elf32 -o $bootObj (Join-Path $root "boot\boot.asm")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& gcc -m32 -ffreestanding -fno-stack-protector -fno-pie -Wall -I (Join-Path $root "include") -O1 -c -o $kernelObj (Join-Path $root "kernel\kernel.c")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& gcc -m32 -nostdlib -T (Join-Path $root "kernel\linker.ld") -o $target $bootObj $kernelObj
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "Built: $target"
