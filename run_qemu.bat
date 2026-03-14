@echo off
REM BTFOS - Run in QEMU (Windows). MIT License.
set KERNEL=btfos.elf
if not exist %KERNEL% (
  echo Building first...
  make
)
qemu-system-i386 -kernel %KERNEL% -serial stdio -display none
