#ifndef FIELDOS_HOLYC_INCLUDE_ERRNO_H
#define FIELDOS_HOLYC_INCLUDE_ERRNO_H

/* Freestanding <errno.h> shim. Single global int; no per-thread errno
 * (the kernel is single-threaded for M3 — Patrol scheduling is M4).
 * The error code constants are POSIX-style; vendored holyc-lang
 * sources read them in error-reporting paths. */

extern int holyc_runtime_errno;
#define errno holyc_runtime_errno

#define EPERM       1
#define ENOENT      2
#define EIO         5
#define EBADF       9
#define EAGAIN     11
#define ENOMEM     12
#define EACCES     13
#define EFAULT     14
#define EBUSY      16
#define EEXIST     17
#define ENODEV     19
#define ENOTDIR    20
#define EISDIR     21
#define EINVAL     22
#define ENOSPC     28
#define EPIPE      32

#endif
