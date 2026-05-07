/* holyc/tools/dump-asm.c
 *
 * Host-side AT&T-corpus capture tool. Drives the vendored holyc-lang
 * front end to its codegen exit (compileToAsm) and writes the
 * resulting AoStr to a file. Used by the `corpus` target in
 * holyc/holyc.mk to produce the spec input for the in-tree x86_64
 * encoder (ADR-0001 §2, ADR-0003 §2). Replaces the encoder-input
 * obtainment that ADR-0001 §3 step 4's superseded "x86.c output sink
 * redirected from fprintf to AoStr" line described — the AoStr is
 * already the sink; this tool just exfiltrates it without going
 * through main.c's emitFile/system($CC) path.
 *
 * Linkage: built against the host hcc object set MINUS main.o (single
 * `main` symbol). main.o's helpers re-declared here:
 *   - is_terminal: defined in main.c; util.h externs it; cli.c and
 *     cctrl.c read it. Define here so the link resolves.
 *   - memoryInit/Release: main.c-local wrappers around three
 *     primitives whose prototypes are public; call the primitives
 *     directly rather than re-declare the wrappers.
 *
 * Out of scope: the cfg/transpile/run/emit-object code paths in
 * main.c. This tool only exercises compileToAst -> compileToAsm.
 */

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "aostr.h"
#include "ast.h"
#include "cctrl.h"
#include "cli.h"
#include "compile.h"
#include "lexer.h"
#include "memory.h"

#ifndef INSTALL_PREFIX
#define INSTALL_PREFIX "/usr/local"
#endif

int is_terminal;

static int dumpOne(const char *infile, const char *outfile) {
    CliArgs args;
    cliArgsInit(&args);
    args.install_dir = INSTALL_PREFIX;
    args.infile = mprintf("%s", infile);

    const char *base = strrchr(infile, '/');
    base = base ? base + 1 : infile;
    const char *dot = strrchr(base, '.');
    int n = dot ? (int)(dot - base) : (int)strlen(base);
    args.infile_no_ext = mprintf("%.*s", n, base);
    args.asm_outfile  = mprintf("%s.s", args.infile_no_ext);
    args.obj_outfile  = mprintf("%s.o", args.infile_no_ext);

    Cctrl *cc = cctrlNew();
    compileToAst(cc, &args, CCF_PRE_PROC);
    AoStr *asmbuf = compileToAsm(cc);
    if (!asmbuf || !asmbuf->data || asmbuf->len == 0) {
        fprintf(stderr, "dump-asm: empty asmbuf for %s\n", infile);
        return 1;
    }

    int fd = open(outfile, O_CREAT|O_TRUNC|O_RDWR, 0644);
    if (fd == -1) {
        fprintf(stderr, "dump-asm: open %s: %s\n", outfile, strerror(errno));
        return 1;
    }
    size_t towrite = asmbuf->len;
    const char *ptr = asmbuf->data;
    while (towrite > 0) {
        ssize_t w = write(fd, ptr, towrite);
        if (w < 0) {
            if (errno == EINTR) continue;
            fprintf(stderr, "dump-asm: write %s: %s\n", outfile, strerror(errno));
            close(fd);
            return 1;
        }
        towrite -= (size_t)w;
        ptr     += w;
    }
    close(fd);
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: %s <input.HC> <output.s>\n", argv[0]);
        return 2;
    }

    is_terminal = isatty(STDOUT_FILENO) && isatty(STDERR_FILENO);

    astMemoryInit();
    lexemeMemoryInit();
    globalArenaInit(4096 * 10);

    int rc = dumpOne(argv[1], argv[2]);

    lexemeMemoryRelease();
    astMemoryRelease();
    globalArenaRelease();
    return rc;
}
