# TS-OS Windows build script
# Requires: Rust (nightly), Make (from MSYS2/Git Bash), xorriso

$ErrorActionPreference = "Stop"
Push-Location $PSScriptRoot

# Build kernel
Push-Location kernel
& cargo build --target x86_64-unknown-none
if ($LASTEXITCODE -ne 0) { Pop-Location; Pop-Location; exit 1 }
Copy-Item target\x86_64-unknown-none\debug\ts-os-kernel kernel -Force
Pop-Location

Write-Host "Kernel built. Run 'make all' (GNU Make) for full ISO, or use WSL."
Pop-Location
