# BTFOS (BoggersTheFish OS) - Build. MIT License.
# Requires: nasm, gcc (32-bit: gcc -m32 or i686-elf-gcc), ld
# Optional: QEMU for run

CC     = gcc
ASM   = nasm
LD    = ld
CFLAGS = -m32 -ffreestanding -fno-stack-protector -fno-pie -Wall -Wextra -I include -O1
ASMFLAGS = -f elf32
LDFLAGS = -m elf_i386 -T kernel/linker.ld -nostdlib

KERNEL_OBJ = boot/boot.o kernel/kernel.o
TARGET     = btfos.elf

.PHONY: all clean run

all: $(TARGET)

$(TARGET): $(KERNEL_OBJ)
	$(CC) -m32 -nostdlib -T kernel/linker.ld -o $@ $^

boot/boot.o: boot/boot.asm
	$(ASM) $(ASMFLAGS) -o $@ $<

kernel/kernel.o: kernel/kernel.c include/btfos_config.h
	$(CC) $(CFLAGS) -c -o $@ kernel/kernel.c

clean:
	rm -f $(KERNEL_OBJ) $(TARGET)

# Run in QEMU (install qemu-system-x86)
run: $(TARGET)
	qemu-system-i386 -kernel $(TARGET) -serial stdio -display none

# Run with VGA visible
run-vga: $(TARGET)
	qemu-system-i386 -kernel $(TARGET) -serial stdio

# Bootable ISO (requires grub-mkrescue)
iso: $(TARGET)
	mkdir -p iso/boot/grub
	cp $(TARGET) iso/boot/btfos.elf
	cp iso/boot/grub/grub.cfg iso/boot/grub/
	grub-mkrescue -o btfos.iso iso
	@echo "btfos.iso ready. Run: qemu-system-i386 -cdrom btfos.iso -serial stdio"
