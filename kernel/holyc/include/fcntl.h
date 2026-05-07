#ifndef FIELDOS_HOLYC_INCLUDE_FCNTL_H
#define FIELDOS_HOLYC_INCLUDE_FCNTL_H

/* Freestanding <fcntl.h> shim. Declarations only.
 *
 * Reach observed in the kernel-resident subset: lexer.c:580 calls
 * open(path, O_RDONLY, 0644) inside lexerLoadFile -- a code path
 * that has no analogue in the in-kernel REPL (which receives source
 * over serial as a string buffer, not from a filesystem). The
 * runtime stub for open() returns -1 unconditionally; gc-sections
 * is expected to strip lexerLoadFile from the kernel ELF since no
 * code path from kmain reaches it. */

#include <sys/types.h>

#define O_RDONLY    0x0000
#define O_WRONLY    0x0001
#define O_RDWR      0x0002
#define O_CREAT     0x0040
#define O_EXCL      0x0080
#define O_TRUNC     0x0200
#define O_APPEND    0x0400
#define O_NONBLOCK  0x0800

int open(const char *path, int flags, ...);
int fcntl(int fd, int cmd, ...);

#endif
