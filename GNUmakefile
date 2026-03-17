MAKEFLAGS += -rR
.SUFFIXES:

override USER_VARIABLE = $(if $(filter $(origin $(1)),default undefined),$(eval override $(1) := $(2)))
$(call USER_VARIABLE,KARCH,x86_64)
$(call USER_VARIABLE,QEMUFLAGS,-m 2G -serial stdio)

override IMAGE_NAME := ts-os-$(KARCH)

.PHONY: all
all: $(IMAGE_NAME).iso

.PHONY: run
run: $(IMAGE_NAME).iso
	qemu-system-$(KARCH) -M q35 -cdrom $(IMAGE_NAME).iso -boot d $(QEMUFLAGS)

.PHONY: run-uefi
run-uefi: edk2-ovmf $(IMAGE_NAME).iso
	qemu-system-$(KARCH) -M q35 \
		-drive if=pflash,unit=0,format=raw,file=edk2-ovmf/ovmf-code-$(KARCH).fd,readonly=on \
		-cdrom $(IMAGE_NAME).iso $(QEMUFLAGS)

edk2-ovmf:
	curl -L https://github.com/osdev0/edk2-ovmf-nightly/releases/latest/download/edk2-ovmf.tar.gz | gunzip | tar -xf -

limine/limine:
	rm -rf limine
	git clone https://github.com/limine-bootloader/limine.git --branch=v10.x-binary --depth=1
	$(MAKE) -C limine

.PHONY: kernel
kernel:
	$(MAKE) -C kernel

$(IMAGE_NAME).iso: limine/limine kernel
	rm -rf iso_root
	mkdir -p iso_root/boot
	cp -v kernel/kernel iso_root/boot/
	mkdir -p iso_root/boot/limine
	cp -v limine.conf iso_root/boot/limine/
	mkdir -p iso_root/EFI/BOOT
	cp -v limine/limine-bios.sys limine/limine-bios-cd.bin limine/limine-uefi-cd.bin iso_root/boot/limine/
	cp -v limine/BOOTX64.EFI iso_root/EFI/BOOT/
	cp -v limine/BOOTIA32.EFI iso_root/EFI/BOOT/
	xorriso -as mkisofs -b boot/limine/limine-bios-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table \
		--efi-boot boot/limine/limine-uefi-cd.bin \
		-efi-boot-part --efi-boot-image --protective-msdos-label \
		iso_root -o $(IMAGE_NAME).iso
	./limine/limine bios-install $(IMAGE_NAME).iso
	rm -rf iso_root

.PHONY: clean
clean:
	$(MAKE) -C kernel clean
	rm -rf iso_root $(IMAGE_NAME).iso

.PHONY: distclean
distclean: clean
	rm -rf limine edk2-ovmf
