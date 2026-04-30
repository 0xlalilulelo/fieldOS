#include <stdint.h>

#include "io.h"
#include "serial.h"

/* COM1 16550 UART. Standard register layout:
 *   +0  RBR/THR (data) when DLAB=0; divisor LSB when DLAB=1
 *   +1  IER         when DLAB=0; divisor MSB when DLAB=1
 *   +2  FCR (write) / IIR (read)
 *   +3  LCR
 *   +4  MCR
 *   +5  LSR
 */
#define COM1 0x3F8

void serial_init(void)
{
	outb(COM1 + 1, 0x00);  /* disable interrupts */
	outb(COM1 + 3, 0x80);  /* DLAB on */
	outb(COM1 + 0, 0x01);  /* divisor low: 115200 / 1 = 115200 baud */
	outb(COM1 + 1, 0x00);  /* divisor high */
	outb(COM1 + 3, 0x03);  /* DLAB off, 8N1 */
	outb(COM1 + 2, 0xC7);  /* FIFO enable, 14-byte threshold, clear */
	outb(COM1 + 4, 0x0B);  /* RTS/DSR set, OUT1/OUT2 set, IRQs disabled */
}

void serial_putc(char c)
{
	while (!(inb(COM1 + 5) & 0x20)) {
		/* spin until THR is empty */
	}
	outb(COM1, (uint8_t)c);
}

void serial_puts(const char *s)
{
	while (*s) {
		serial_putc(*s++);
	}
}
