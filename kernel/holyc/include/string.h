#ifndef FIELDOS_HOLYC_INCLUDE_STRING_H
#define FIELDOS_HOLYC_INCLUDE_STRING_H

/* Freestanding <string.h> shim. Pulls in the runtime's own
 * declarations (memcpy / memmove / memset / strlen / strcmp / strchr
 * / strdup) and adds the rest of the surface vendored holyc-lang
 * sources reach for. Functions declared but not yet defined in
 * runtime.c surface as undefined symbols at link time — that is M3-B
 * candidate B's discovery deliverable. */

#include <stddef.h>
#include "../runtime.h"

int    memcmp(const void *a, const void *b, size_t n);
int    strncmp(const char *a, const char *b, size_t n);
int    strncasecmp(const char *a, const char *b, size_t n);
char  *strncpy(char *dst, const char *src, size_t n);
char  *strcpy(char *dst, const char *src);
char  *strcat(char *dst, const char *src);
char  *strncat(char *dst, const char *src, size_t n);
char  *strstr(const char *haystack, const char *needle);
char  *strrchr(const char *s, int c);
char  *strndup(const char *s, size_t n);
char  *strerror(int errnum);

#endif
