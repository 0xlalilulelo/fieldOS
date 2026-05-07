#ifndef FIELDOS_HOLYC_INCLUDE_STDIO_H
#define FIELDOS_HOLYC_INCLUDE_STDIO_H

/* Freestanding <stdio.h> shim. Pulls in the runtime's printf /
 * snprintf / vsnprintf and declares the file-IO surface as opaque
 * stubs; the kernel has no files in M3 (ramfs is M9 per phase-0.md).
 * Vendored holyc-lang sources that touch file IO will surface here
 * at link time as undefined symbols — M3-B candidate B's discovery
 * deliverable. */

#include <stddef.h>
#include <stdarg.h>
#include "../runtime.h"

typedef struct __holyc_file FILE;

extern FILE *stdin;
extern FILE *stdout;
extern FILE *stderr;

#define EOF (-1)
#define BUFSIZ 4096
#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

int    fprintf(FILE *fp, const char *fmt, ...) __attribute__((format(printf, 2, 3)));
int    vfprintf(FILE *fp, const char *fmt, va_list ap);
int    fputs(const char *s, FILE *fp);
int    fputc(int c, FILE *fp);
int    fgetc(FILE *fp);
char  *fgets(char *buf, int n, FILE *fp);
size_t fread(void *p, size_t s, size_t n, FILE *fp);
size_t fwrite(const void *p, size_t s, size_t n, FILE *fp);
FILE  *fopen(const char *path, const char *mode);
int    fclose(FILE *fp);
int    fflush(FILE *fp);
int    feof(FILE *fp);
int    fseek(FILE *fp, long off, int whence);
long   ftell(FILE *fp);
void   perror(const char *s);
int    fileno(FILE *fp);
int    putchar(int c);
int    puts(const char *s);

#endif
