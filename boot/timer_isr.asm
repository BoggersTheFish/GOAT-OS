; BTFOS - Timer IRQ0 stub for IDT. Calls C handler then iret.
; NASM, 32-bit. MIT License.

bits 32
section .text
global irq0_entry
extern timer_irq_handler

irq0_entry:
    push byte 0       ; error code (dummy)
    push byte 32      ; int number (IRQ0)
    pusha
    push ds
    push es
    push fs
    push gs
    mov ax, 0x10       ; kernel data segment
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    call timer_irq_handler
    pop gs
    pop fs
    pop es
    pop ds
    popa
    add esp, 8        ; drop int no + error code
    iret
