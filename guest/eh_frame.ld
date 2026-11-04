/* Added to lld's default layout: a symbol at the start of .eh_frame and the end marker
   that libgcc's unwinder needs (crtbegin.o and crtend.o do this in a normal GCC link). */
SECTIONS
{
  .eh_frame : ALIGN(8)
  {
    __eh_frame_start = .;
    KEEP(*(.eh_frame))
    LONG(0)
  }
}
INSERT AFTER .rodata;
