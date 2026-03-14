/* BTFOS - Multiboot 1 boot (GAS). Alternative to boot.asm. MIT License. */
.code32
.section .multiboot
.align 4
.long 0x1BADB002
.long 0x00000003
.long -(0x1BADB002 + 0x00000003)

.section .bss
.align 16
stack_bottom:
.space 16384
stack_top:

.section .text
.global _start
.extern kernel_main
_start:
    movl $stack_top, %esp
    pushl %ebx
    pushl %eax
    cli
    call kernel_main
1:  hlt
    jmp 1b
