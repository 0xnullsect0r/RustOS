/* rustos-link.x  —  Linker script for RustOS userspace programs
 *
 * Load address: 0x0040_0000 (4 MiB) — well above the kernel's identity-mapped area.
 * The ELF process loader in RustOS maps PT_LOAD segments to their virtual addresses.
 */

ENTRY(_start)

SECTIONS {
    . = 0x400000;

    .text : {
        *(.text._start)
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .data : {
        *(.data .data.*)
    }

    .bss : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.note .note.*)
        *(.comment)
    }
}
