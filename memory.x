MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 256K
  RAM : ORIGIN = 0x20000000, LENGTH = 32K
}

/* Increase stack from default to 8KB — init() allocates ~1KB+
   stack buffers (Options::load/save) and has many locals.
   8KB gives comfortable headroom for RTIC tasks + interrupts. */
STACK_SIZE = 8192;
