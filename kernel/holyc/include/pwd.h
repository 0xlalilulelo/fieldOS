#ifndef FIELDOS_HOLYC_INCLUDE_PWD_H
#define FIELDOS_HOLYC_INCLUDE_PWD_H

/* Freestanding <pwd.h> shim. The vendored subset #includes <pwd.h>
 * but no observed call site reaches getpwuid / getpwnam in the four
 * .c files we link (cctrl.c, lexer.c, prslib.c, prsutil.c, list.c).
 * Declarations are kept minimal so any future real call surfaces as
 * an undefined symbol at link time rather than compiling silently. */

#include <sys/types.h>

struct passwd {
	char  *pw_name;
	char  *pw_passwd;
	uid_t  pw_uid;
	gid_t  pw_gid;
	char  *pw_gecos;
	char  *pw_dir;
	char  *pw_shell;
};

struct passwd *getpwuid(uid_t uid);
struct passwd *getpwnam(const char *name);

#endif
