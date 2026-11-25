/*
 * Differential driver for the unmodified TARGET_PC aprintf implementation.
 */
#include <stdarg.h>
#include <stdio.h>
#include <string.h>

#include "libc64/aprintf.h"

static char sink[256];
static int sink_len;

static void* collect(void* arg, const char* bytes, int length) {
    aprout_func_t* func = (aprout_func_t*)arg;
    (void)func;
    memcpy(sink, bytes, (size_t)length);
    sink[length] = '\0';
    sink_len = length;
    return arg;
}

static int run(const char* fmt, ...) {
    aprout_func_t func = {collect};
    va_list ap;
    va_start(ap, fmt);
    int result = vaprintf(&func, fmt, ap);
    va_end(ap);
    printf("%d %d %s\n", result, sink_len, sink);
    return result;
}

#include "../../../src/static/libc64/aprintf.c"

int main(void) {
    run("oak:%s:%d:%u:%04x:%.2f", "leaf", -7, 42u, 0x2au, 3.5);
    run("percent:%%:%ld", (long)-12);
    return 0;
}
