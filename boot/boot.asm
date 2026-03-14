; BTFOS (BoggersTheFish OS) - Multiboot 1 boot sector
; NASM, 32-bit. Loaded by GRUB or QEMU -kernel. Runs in protected mode.
; MIT License

bits 32
section .multiboot
align 4
multiboot_header:
    dd 0x1BADB002              ; magic
    dd 0x00000003               ; flags: align modules, mem_info
    dd -(0x1BADB002 + 0x00000003) ; checksum

section .bss
align 16
stack_bottom:
    resb 16384                 ; 16KiB stack
stack_top:

section .text
global _start
extern kernel_main

_start:
    mov esp, stack_top
    push eax                    ; multiboot magic
    push ebx                    ; multiboot info ptr
    cli
    call kernel_main
.hang:
    hlt
    jmp .hang
