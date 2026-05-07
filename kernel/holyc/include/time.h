#ifndef FIELDOS_HOLYC_INCLUDE_TIME_H
#define FIELDOS_HOLYC_INCLUDE_TIME_H

/* Freestanding <time.h> shim. Declarations only.
 *
 * The vendored subset's cctrl.c #includes <time.h> (transitively
 * with sys/time.h) but the observed reach is limited to gettimeofday
 * on the sys/time path. Declarations here are kept minimal; any
 * actual call surfaces as undefined at link time. */

#include <sys/types.h>

#ifndef CLOCKS_PER_SEC
#define CLOCKS_PER_SEC 1000000L
#endif

typedef long clock_t;

struct tm {
	int tm_sec;
	int tm_min;
	int tm_hour;
	int tm_mday;
	int tm_mon;
	int tm_year;
	int tm_wday;
	int tm_yday;
	int tm_isdst;
};

time_t   time(time_t *t);
clock_t  clock(void);
struct tm *localtime(const time_t *t);
struct tm *gmtime(const time_t *t);
char    *asctime(const struct tm *t);
char    *ctime(const time_t *t);

#endif
