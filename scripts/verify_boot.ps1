# BTFOS - Build, run in QEMU, assert "BTFOS Ready" in serial output.
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot\..
& $PSScriptRoot\check_deps.ps1
if (Get-Command make -ErrorAction SilentlyContinue) {
    make clean 2>$null; make
} else {
    & $PSScriptRoot\..\build.ps1
}
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$k = Join-Path $root "btfos.elf"
$job = Start-Job -ScriptBlock { param($k) & qemu-system-i386 -kernel $k -serial stdio -display none 2>&1 } -ArgumentList $k
Wait-Job $job -Timeout 8
$out = Receive-Job $job
$out | Set-Content (Join-Path $PSScriptRoot "..\serial.log")
if ($out -notmatch "BTFOS Ready") { Write-Host "Boot verification failed."; $out; exit 1 }
Write-Host "BTFOS boot verified."
