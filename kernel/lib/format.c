#include "lib/format.h"
#include "arch/x86_64/serial.h"

void format_dec(uint64_t v)
{
	if (v == 0) {
		serial_putc('0');
		return;
	}
	char buf[21];
	int i = 20;
	buf[i] = '\0';
	while (v > 0) {
		buf[--i] = '0' + (char)(v % 10);
		v /= 10;
	}
	serial_puts(&buf[i]);
}
