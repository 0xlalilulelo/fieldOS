#ifndef FIELDOS_ARCH_X86_64_SERIAL_H
#define FIELDOS_ARCH_X86_64_SERIAL_H

void serial_init(void);
void serial_putc(char c);
void serial_puts(const char *s);
char serial_getc(void);

#endif
