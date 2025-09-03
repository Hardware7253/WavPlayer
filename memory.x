/* memory.x - Linker script for STM32F411CEU6 */

MEMORY
{
  FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 512k
  RAM (rw) : ORIGIN = 0x20000000, LENGTH = 128k 
}
