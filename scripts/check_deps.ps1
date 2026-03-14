# BTFOS - Verify build/run dependencies. Exit 0 if all present.
$missing = @()
if (-not (Get-Command nasm -ErrorAction SilentlyContinue)) { $missing += "nasm" }
if (-not (Get-Command gcc -ErrorAction SilentlyContinue)) { $missing += "gcc" }
# 32-bit: gcc -m32 --version often works if multilib is installed
$null = & gcc -m32 --version 2>&1
if ($LASTEXITCODE -ne 0) { $missing += "gcc-m32" }
$q = Get-Command qemu-system-i386 -ErrorAction SilentlyContinue
if (-not $q) { $q = Get-Command qemu-system-x86 -ErrorAction SilentlyContinue }
if (-not $q) { $missing += "qemu-system-i386" }
if ($missing.Count -gt 0) {
    Write-Host "Missing: $($missing -join ' ')"
    Write-Host "Install: nasm, gcc (32-bit), qemu-system-x86"
    exit 1
}
Write-Host "OK: nasm gcc qemu"
exit 0
