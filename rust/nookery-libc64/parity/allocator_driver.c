/*
 * Differential driver for the unmodified libc64 boot allocator.
 *
 * The source include below is deliberately the real upstream implementation.
 * Only the OS lock/report calls are stubbed so the allocator can run as a
 * deterministic host oracle.
 */
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include "libc64/__osMalloc.h"

static int panic_count;
static int panic_line;

void OSReport(const char* fmt, ...) {
    (void)fmt;
}

void OSPanic(const char* file, int line, const char* message, ...) {
    (void)file;
    (void)message;
    panic_count++;
    panic_line = line;
}

OSTime osGetTime(void) {
    return 0;
}

int osGetThreadId(OSThread* thread) {
    (void)thread;
    return 0;
}

void osCreateMesgQueue(OSMessageQueue* queue, OSMessage* msg, int count) {
    (void)queue;
    (void)msg;
    (void)count;
}

int osSendMesg(OSMessageQueue* queue, OSMessage msg, int flags) {
    (void)queue;
    (void)msg;
    (void)flags;
    return 0;
}

int osRecvMesg(OSMessageQueue* queue, OSMessage* msg, int flags) {
    (void)queue;
    (void)msg;
    (void)flags;
    return 0;
}

#include "../../../src/static/libc64/__osMalloc.c"

static unsigned char storage[4096] __attribute__((aligned(256)));

static int block_offset(const OSMemBlock* block) {
    return block == NULL ? -1 : (int)((const unsigned char*)block - storage);
}

static int payload_offset(const void* ptr) {
    return ptr == NULL ? -1 : (int)((const unsigned char*)ptr - storage);
}

static void print_state(const char* tag, OSArena* arena, const void* result) {
    u32 max_free = 0;
    u32 total_free = 0;
    u32 used = 0;
    __osGetFreeArena(arena, &max_free, &total_free, &used);
    printf("%s result=%d panic=%d:%d head=%d free=%lu:%lu used=%lu\n",
           tag, payload_offset(result), panic_count, panic_line,
           block_offset(arena->head), (unsigned long)max_free,
           (unsigned long)total_free, (unsigned long)used);
    for (OSMemBlock* block = arena->head; block != NULL; block = block->next) {
        printf("  block=%d next=%d prev=%d size=%ld free=%d magic=%d\n",
               block_offset(block), block_offset(block->next),
               block_offset(block->prev), (long)block->size,
               block->free, (int)block->magic);
    }
}

int main(void) {
    OSArena arena;
    void* first;
    void* aligned;
    void* zero;
    void* reverse;

    __osMallocInit(&arena, storage + 3, 2048);
    print_state("init", &arena, NULL);

    first = __osMalloc(&arena, 17);
    print_state("malloc17", &arena, first);

    aligned = __osMallocAlign(&arena, 33, 256);
    print_state("align33_256", &arena, aligned);

    zero = __osMalloc(&arena, 0);
    print_state("malloc0", &arena, zero);

    print_state("exhausted", &arena, __osMalloc(&arena, 0x7fffffffU));

    __osFree(&arena, first);
    print_state("free_first", &arena, NULL);

    __osFree(&arena, first);
    print_state("double_first", &arena, NULL);

    __osFree(&arena, (unsigned char*)aligned + 1);
    print_state("misaligned", &arena, NULL);

    __osFree(&arena, aligned);
    print_state("free_aligned", &arena, NULL);

    reverse = __osMallocR(&arena, 64);
    print_state("malloc_reverse64", &arena, reverse);

    __osFree(&arena, NULL);
    print_state("free_null", &arena, NULL);

    return 0;
}
