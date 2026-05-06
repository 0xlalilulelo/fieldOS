#ifndef FIELDOS_LIB_FORMAT_H
#define FIELDOS_LIB_FORMAT_H

#include <stdint.h>

/* Print v to serial as unsigned decimal, no leading zeros, no
 * separators. Prints "0" for zero. Panic-path safe: no allocation,
 * no globals, single 21-byte stack buffer. */
void format_dec(uint64_t v);

#endif
