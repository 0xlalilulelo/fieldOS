#ifndef FIELDOS_HOLYC_INCLUDE_UNISTD_H
#define FIELDOS_HOLYC_INCLUDE_UNISTD_H

/* Freestanding <unistd.h> shim. Declarations only. The kernel has no
 * processes, no file descriptors, no tty in M3 — this file's job is
 * to satisfy the parser when vendored sources #include <unistd.h>
 * and surface symbol-level usage as undefined at link time, the same
 * pattern <stdio.h> follows for fopen/fread/fwrite.
 *
 * Reach observed in the kernel-resident subset: ast.c uses
 * isatty(STDOUT_FILENO) to gate ANSI color escapes in its
 * pretty-printer (5 sites). main.c and transpiler.c also touch
 * unistd, but they are not in HOLYC_KERNEL_SRCS. */

#include <stddef.h>
#include <sys/types.h>

#define STDIN_FILENO  0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2

int     isatty(int fd);
ssize_t read(int fd, void *buf, size_t n);
ssize_t write(int fd, const void *buf, size_t n);
int     close(int fd);
off_t   lseek(int fd, off_t off, int whence);
int     unlink(const char *path);
int     access(const char *path, int mode);
int     dup(int fd);
int     dup2(int oldfd, int newfd);
unsigned sleep(unsigned seconds);
int     usleep(unsigned useconds);
pid_t   getpid(void);
pid_t   fork(void);
int     execv(const char *path, char *const argv[]);

#endif
