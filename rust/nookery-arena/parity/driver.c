/*
 * Freestanding 32-bit differential driver for the UNMODIFIED upstream sources
 * src/TwoHeadArena.c and src/gamealloc.c at pin a81c1aaf.
 * Prints one line per observable step; the Rust binary prints the same lines.
 */
#include "TwoHeadArena.h"
#include "gamealloc.h"

/* The header's own annotations only hold at 32-bit width; fail the build
 * instead of emitting a transcript from a 64-bit model. */
_Static_assert(sizeof(GameAllocList) == 0x10, "GameAllocList must be 0x10 bytes");
_Static_assert(sizeof(GameAlloc) == 0x14, "GameAlloc must be 0x14 bytes");
_Static_assert(sizeof(TwoHeadArena) == 0x10, "TwoHeadArena must be 0x10 bytes");
_Static_assert(sizeof(void*) == 4, "harness must be built -m32");

/* ---- syscall-only stdlib ---- */
static long sys_write(int fd, const void* buf, unsigned long len) {
    long r;
    __asm__ volatile("int $0x80" : "=a"(r) : "a"(4), "b"(fd), "c"(buf), "d"(len) : "memory");
    return r;
}
static void sys_exit(int code) {
    __asm__ volatile("int $0x80" :: "a"(1), "b"(code) : "memory");
    __builtin_unreachable();
}
static void ps(const char* s) { unsigned long n = 0; while (s[n]) n++; sys_write(1, s, n); }
static void pc(char c) { sys_write(1, &c, 1); }
static void ph(unsigned int v) {
    const char* hex = "0123456789abcdef";
    char out[8];
    for (int i = 0; i < 8; i++) out[i] = hex[(v >> (28 - 4 * i)) & 0xF];
    sys_write(1, out, 8);
}
static void pd(int v) {
    char out[12]; int n = 0;
    unsigned int m;
    if (v < 0) { pc('-'); m = (unsigned int)-v; } else { m = (unsigned int)v; }
    if (m == 0) { pc('0'); return; }
    while (m) { out[n++] = '0' + (m % 10); m /= 10; }
    while (n) pc(out[--n]);
}
void bzero(void* p, unsigned long n) { volatile char* c = p; while (n--) *c++ = 0; }
void* memset(void* p, int v, unsigned long n) { volatile char* c = p; while (n--) *c++ = (char)v; return p; }

/* ---- bump allocator: never reuses a freed block ---- */
static char heap[64 * 1024];
static unsigned int heap_cursor = 0;
void* malloc(size_t size) {
    unsigned int at = heap_cursor;
    if (at + size > sizeof heap) return (void*)0;
    heap_cursor = at + (unsigned int)size;
    return (void*)(heap + at);
}
void free(void* ptr) { (void)ptr; }

static void pr(void* p);

static void tha_state(const char* tag, TwoHeadArena* t) {
    ps(tag); ps(" tail="); ph((unsigned int)(size_t)t->tail_p);
    ps(" free="); pd(THA_getFreeBytes(t));
    ps(" free16="); pd(THA_getFreeBytes16(t));
    ps(" crash="); pd(THA_isCrash(t));
    ps(" head="); ph((unsigned int)(size_t)t->head_p);
    ps("\n");
}
static void ga_state(const char* tag, GameAlloc* g) {
    ps(tag);
    ps(" head.next="); pr(g->head.next);
    ps(" head.prev="); pr(g->head.prev);
    ps(" tail="); pr(g->tail);
    ps("\n");
}
/* Print a heap-relative offset, or HEAD for the &gamealloc->head sentinel
 * (a stack address, so it has no heap offset). */
static void pr(void* p) {
    if (p == (void*)0) { ps("NULL"); return; }
    char* c = (char*)p;
    if (c < heap || c >= heap + sizeof heap) { ps("HEAD"); return; }
    ph((unsigned int)(c - heap));
}

int main(void);

void _start(void) {
    int code = main();
    sys_exit(code);
}

int main(void) {
    /* ---------- THA: arithmetic only, synthetic addresses ---------- */
    TwoHeadArena t;
    THA_ct(&t, (char*)0x2000, 0x100);
    tha_state("ct_2000_100", &t);

    ps("a16_20="); ph((unsigned int)(size_t)THA_alloc16(&t, 0x20)); ps("\n");
    tha_state("after_a16_20", &t);
    ps("a16_11="); ph((unsigned int)(size_t)THA_alloc16(&t, 0x11)); ps("\n");
    ps("a16_0="); ph((unsigned int)(size_t)THA_alloc16(&t, 0)); ps("\n");
    tha_state("after_zero", &t);

    THA_init(&t);
    tha_state("reinit", &t);
    ps("aa_40_mask~1f="); ph((unsigned int)(size_t)THA_allocAlign(&t, 0x40, ~0x1F)); ps("\n");
    ps("aa_41_mask~1f="); ph((unsigned int)(size_t)THA_allocAlign(&t, 0x41, ~0x1F)); ps("\n");
    ps("aa_10_mask-1="); ph((unsigned int)(size_t)THA_allocAlign(&t, 0x10, -1)); ps("\n");
    ps("aa_10_mask-2="); ph((unsigned int)(size_t)THA_allocAlign(&t, 0x10, -2)); ps("\n");
    tha_state("after_aa", &t);

    /* unaligned base: head alignment asymmetry */
    THA_ct(&t, (char*)0x2008, 0x100);
    tha_state("ct_2008_100", &t);
    THA_ct(&t, (char*)0x2001, 0x100);
    ps("gfb_~1f="); pd(THA_getFreeBytesAlign(&t, ~0x1F)); ps("\n");
    ps("gfb_-16="); pd(THA_getFreeBytesAlign(&t, -16)); ps("\n");
    ps("gfb_-1="); pd(THA_getFreeBytesAlign(&t, -1)); ps("\n");

    /* exhaustion + collision */
    THA_ct(&t, (char*)0x2000, 0x100);
    ps("exact="); ph((unsigned int)(size_t)THA_alloc16(&t, 0x100)); ps("\n");
    tha_state("at_exact", &t);
    ps("over="); ph((unsigned int)(size_t)THA_alloc16(&t, 0x10)); ps("\n");
    tha_state("after_over", &t);

    /* 32-bit underflow wrap */
    THA_ct(&t, (char*)0x10, 0x10);
    ps("wrap="); ph((unsigned int)(size_t)THA_alloc16(&t, 0x30)); ps("\n");
    tha_state("after_wrap", &t);

    /* dt zeroes everything */
    THA_dt(&t);
    tha_state("dt", &t);

    /* end address that wraps the 32-bit space */
    THA_ct(&t, (char*)0xFFFFFF00u, 0x200);
    tha_state("ct_wrap_end", &t);

    /* fixed-seed pseudo-random interleaving: same LCG and call order as the
     * Rust replay in tests/parity.rs, so the whole arithmetic path is compared
     * over many operand shapes rather than the hand-picked cases above. */
    THA_ct(&t, (char*)0x3000, 0x400);
    {
        unsigned int seed = 0x1234567u;
        for (int i = 0; i < 24; i++) {
            seed = seed * 1103515245u + 12345u;
            unsigned int r = (seed >> 16) & 0x7FFF;
            unsigned int siz = (r & 0x7) == 0 ? 0 : (r & 0x7F) + 1;
            int mask;
            switch (r & 3) {
                case 0: mask = -1; break;
                case 1: mask = -16; break;
                case 2: mask = ~0x1F; break;
                default: mask = -2; break;
            }
            ps("sim="); pd(i);
            ps(" siz="); pd((int)siz);
            ps(" mask="); pd(mask);
            ps(" ptr="); ph((unsigned int)(size_t)THA_allocAlign(&t, siz, mask));
            ps(" free="); pd(THA_getFreeBytes(&t));
            ps(" free16="); pd(THA_getFreeBytes16(&t));
            ps(" crash="); pd(THA_isCrash(&t));
            ps("\n");
        }
        tha_state("sim_end", &t);
    }

    /* ---------- GameAlloc ---------- */
    /* Pad so the first node lands at heap offset 0x10, matching the Rust test. */
    heap_cursor = 0x10;

    GameAlloc g;
    gamealloc_init(&g);
    ga_state("ga_init", &g);

    void* p1 = gamealloc_malloc(&g, 0x20);
    ps("ga_m1="); pr(p1); ps("\n");
    ga_state("ga_after_m1", &g);
    void* p2 = gamealloc_malloc(&g, 0x10);
    ps("ga_m2="); pr(p2); ps("\n");
    void* p3 = gamealloc_malloc(&g, 0);
    ps("ga_m3="); pr(p3); ps("\n");
    ga_state("ga_after_m3", &g);

    /* node headers, relative to heap */
    for (int i = 0; i < 3; i++) {
        GameAllocList* n = (GameAllocList*)((char*)(i == 0 ? p1 : i == 1 ? p2 : p3) - sizeof(GameAllocList));
        ps("node="); pd(i);
        ps(" next="); pr(n->next);
        ps(" prev="); pr(n->prev);
        ps(" size="); pd((int)n->alloc_size);
        ps("\n");
    }

    gamealloc_free(&g, p2);
    ga_state("ga_after_free_m2", &g);
    ps("node0_next_after_free="); pr(((GameAllocList*)((char*)p1 - sizeof(GameAllocList)))->next); ps("\n");
    ps("node2_prev_after_free="); pr(((GameAllocList*)((char*)p3 - sizeof(GameAllocList)))->prev); ps("\n");

    gamealloc_free(&g, (void*)0);
    ga_state("ga_after_free_null", &g);

    void* p4 = gamealloc_malloc(&g, 0x40);
    ps("ga_m4="); pr(p4); ps("\n");
    ga_state("ga_after_m4", &g);

    gamealloc_cleanup(&g);
    ga_state("ga_cleanup", &g);

    void* p5 = gamealloc_malloc(&g, 0x8);
    ps("ga_after_cleanup_m5="); pr(p5); ps("\n");
    ga_state("ga_after_m5", &g);

    /* exhausted allocator: NULL and untouched list */
    heap_cursor = sizeof heap;
    ps("ga_exhausted="); pr(gamealloc_malloc(&g, 0x10)); ps("\n");
    ga_state("ga_after_exhausted", &g);

    sys_exit(0);
}
