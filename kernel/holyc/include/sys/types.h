#ifndef FIELDOS_HOLYC_INCLUDE_SYS_TYPES_H
#define FIELDOS_HOLYC_INCLUDE_SYS_TYPES_H

/* Freestanding <sys/types.h> shim. Vendored holyc-lang headers
 * (aostr.h, lexer.h, cctrl.h) reach for ssize_t; defining it here
 * lets those headers parse without modifying the vendored tree. */

#include <stddef.h>

typedef long           ssize_t;
typedef long           off_t;
typedef int            pid_t;
typedef unsigned int   mode_t;
typedef unsigned int   uid_t;
typedef unsigned int   gid_t;
typedef long           time_t;

#endif
