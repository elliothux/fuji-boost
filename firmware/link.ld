/* Raw image loaded at physical 0x00100000 with the MMU off; see src/layout.rs. */
ENTRY(_start)

MEMORY
{
  IMAGE (rwx) : ORIGIN = 0x00100000, LENGTH = 8M
}

SECTIONS
{
  .text : {
    KEEP(*(.text.boot))
    *(.text .text.*)
  } > IMAGE

  .rodata : ALIGN(8) {
    *(.rodata .rodata.*)
  } > IMAGE

  .ARM.exidx : {
    *(.ARM.exidx .ARM.exidx.* .gnu.linkonce.armexidx.*)
  } > IMAGE

  .data : ALIGN(8) {
    *(.data .data.*)
  } > IMAGE

  .bss (NOLOAD) : ALIGN(8) {
    __bss_start = .;
    *(.bss .bss.* COMMON)
    . = ALIGN(8);
    __bss_end = .;
  } > IMAGE

  __image_end = .;
  __stack_top = 0x00F00000;
  __exception_stack_top = 0x00F10000;
}
