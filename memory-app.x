/* Application: slot A, the 120 KiB after the bootloader.
 *
 * Must agree with `fwimage::layout`, which is the single source of
 * truth; its `layout_is_consistent` test checks the arithmetic.
 *
 *   0x08000000  bootloader  16K
 *   0x08004000  slot A     120K   <- this
 *   0x08022000  slot B     120K   staged update
 *
 * The vector table therefore lands at 0x08004000, and the bootloader
 * points VTOR here before jumping.
 *
 * NOT named memory.x, and that is deliberate. `cortex-m-rt`'s link.x
 * does `INCLUDE memory.x`, and the linker searches its working
 * directory — the workspace root — before any -L path. A file called
 * memory.x here would therefore be picked up by *every* crate in the
 * workspace, bootloader included, silently overriding the one that
 * crate ships. build.rs copies this into OUT_DIR under the name the
 * linker wants.
 *
 * No STACK_SIZE line: cortex-m-rt does not read one. It honours
 * `_stack_start`, which defaults to the end of RAM, so the stack starts
 * at 0x20008000 and grows down into whatever .data and .bss leave. The
 * STACK_SIZE = 8192 that used to sit in this file never had any effect.
 */
MEMORY
{
  FLASH : ORIGIN = 0x08004000, LENGTH = 120K
  RAM   : ORIGIN = 0x20000000, LENGTH = 32K
}
