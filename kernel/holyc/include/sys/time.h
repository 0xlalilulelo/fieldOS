#ifndef FIELDOS_HOLYC_INCLUDE_SYS_TIME_H
#define FIELDOS_HOLYC_INCLUDE_SYS_TIME_H

/* Freestanding <sys/time.h> shim.
 *
 * Reach observed in the kernel-resident subset: cctrl.c:109 calls
 * gettimeofday(&tm, NULL) for diagnostic timestamping in error
 * paths. The runtime stub returns 0 / fills tv_sec=tv_usec=0; the
 * caller's "diagnostic prints elapsed time" output is therefore
 * meaningless in the kernel, but the call does not panic and the
 * compile path is satisfied. */

#include <sys/types.h>

struct timeval {
	time_t       tv_sec;
	long         tv_usec;
};

struct timezone {
	int tz_minuteswest;
	int tz_dsttime;
};

int gettimeofday(struct timeval *tv, struct timezone *tz);

#endif
