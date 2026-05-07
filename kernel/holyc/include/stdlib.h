#ifndef FIELDOS_HOLYC_INCLUDE_STDLIB_H
#define FIELDOS_HOLYC_INCLUDE_STDLIB_H

/* Freestanding <stdlib.h> shim. Pulls in the runtime's malloc /
 * free and declares the rest. realloc / calloc / qsort / exit /
 * atoi / atol / getenv / abort surface as undefined at link time
 * if vendored sources reach for them — M3-B candidate B's
 * discovery deliverable. */

#include <stddef.h>
#include "../runtime.h"

#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1
#define RAND_MAX     0x7fffffff

void          *realloc(void *p, size_t bytes);
void          *calloc(size_t nmemb, size_t size);
_Noreturn void  exit(int status);
_Noreturn void  abort(void);
int             atoi(const char *s);
long            atol(const char *s);
long long       atoll(const char *s);
double          atof(const char *s);
long            strtol(const char *s, char **endp, int base);
unsigned long   strtoul(const char *s, char **endp, int base);
long long       strtoll(const char *s, char **endp, int base);
unsigned long long strtoull(const char *s, char **endp, int base);
long double     strtold(const char *s, char **endp);
double          strtod(const char *s, char **endp);
char           *getenv(const char *name);
int             system(const char *cmd);
void            qsort(void *base, size_t nmemb, size_t size,
                      int (*cmp)(const void *, const void *));
int             rand(void);
void            srand(unsigned int seed);
int             abs(int x);
long            labs(long x);

#endif
