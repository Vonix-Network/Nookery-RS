/*
 * Differential driver for the unmodified libc64 sprintf wrapper.  The
 * production xprintf engine is not part of this bounded source slice, so the
 * oracle supplies the TARGET_PC formatting engine while executing the real
 * wrapper and its callback/termination logic.
 */
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>

static int _Printf(void* (*callback)(void*, const char*, int), void* arg,
                   const char* fmt, va_list ap) {
    va_list count_args;
    va_copy(count_args, ap);
    int length = vsnprintf(NULL, 0, fmt, count_args);
    va_end(count_args);
    if (length < 0) {
        return length;
    }
    char* buffer = malloc((size_t)length + 1);
    if (buffer == NULL) {
        return -1;
    }
    va_list format_args;
    va_copy(format_args, ap);
    int written = vsnprintf(buffer, (size_t)length + 1, fmt, format_args);
    va_end(format_args);
    if (written >= 0 && callback(arg, buffer, written) == NULL) {
        written = -1;
    }
    free(buffer);
    return written;
}

#include "../../../src/static/libc64/sprintf.c"

int main(void) {
    char buffer[128];
    int result = sprintf(buffer, "oak:%s:%d:%u:%04x:%.2f", "leaf", -7, 42u,
                         0x2au, 3.5);
    printf("%d %s\n", result, buffer);
    result = sprintf(buffer, "percent:%%:%ld", (long)-12);
    printf("%d %s\n", result, buffer);
    return 0;
}
