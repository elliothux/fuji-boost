@ Reset entry for every core. The emulator starts all four A7 cores at the
@ image entry in SVC mode; cores 1-3 park in WFI with no interrupt targeted
@ at them, so the board's idle skip never schedules them again.
    .section .text.boot, "ax"
    .arm
    .global _start
_start:
    cpsid   if
    mrc     p15, 0, r0, c0, c0, 5
    ands    r0, r0, #3
    bne     park

    ldr     r0, =vectors
    mcr     p15, 0, r0, c12, c0, 0
    @ SCTLR: low vectors via VBAR (clear V), no alignment faults (clear A).
    mrc     p15, 0, r0, c1, c0, 0
    bic     r0, r0, #(1 << 13)
    bic     r0, r0, #(1 << 1)
    mcr     p15, 0, r0, c1, c0, 0
    @ CPACR: full access to cp10/cp11, then FPEXC.EN.
    mrc     p15, 0, r0, c1, c0, 2
    orr     r0, r0, #(0xf << 20)
    mcr     p15, 0, r0, c1, c0, 2
    isb
    mov     r0, #0x40000000
    vmsr    fpexc, r0

    ldr     r0, =__exception_stack_top
    cps     #0x17
    mov     sp, r0
    cps     #0x1b
    mov     sp, r0
    cps     #0x12
    mov     sp, r0
    cps     #0x11
    mov     sp, r0
    cps     #0x13
    ldr     sp, =__stack_top

    ldr     r0, =__bss_start
    ldr     r1, =__bss_end
    mov     r2, #0
1:  cmp     r0, r1
    strlo   r2, [r0], #4
    blo     1b

    bl      fuji_main
park:
    wfi
    b       park

    .balign 32
vectors:
    b       _start
    b       vector_undefined
    b       vector_svc
    b       vector_prefetch
    b       vector_data
    b       park
    b       vector_irq
    b       vector_fiq

vector_undefined:
    mov     r0, #1
    b       vector_common
vector_svc:
    mov     r0, #2
    b       vector_common
vector_prefetch:
    mov     r0, #3
    b       vector_common
vector_data:
    mov     r0, #4
    b       vector_common
vector_irq:
    mov     r0, #6
    b       vector_common
vector_fiq:
    mov     r0, #7
vector_common:
    mov     r1, lr
    mrs     r2, spsr
    bl      fuji_exception
    b       park
    .ltorg
