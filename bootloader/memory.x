/* Bootloader: the first 16 KiB of flash.
 *
 * Must agree with `fwimage::layout` — that module is the single source
 * of truth and its `layout_is_consistent` test checks the arithmetic.
 * The numbers are repeated here only because a linker script cannot
 * read Rust constants.
 *
 *   0x08000000  bootloader  16K   <- this
 *   0x08004000  slot A     120K   application
 *   0x08022000  slot B     120K   staged update
 *
 * RAM is given in full: the bootloader runs alone, and the application
 * re-establishes its own stack from its own vector table on entry.
 */
MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 16K
  RAM   : ORIGIN = 0x20000000, LENGTH = 32K
}
