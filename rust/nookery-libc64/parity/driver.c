/*
 * Differential driver for the unmodified libc64 qrand oracle.
 *
 * The build command supplies src/static/libc64/qrand.c directly.  This file
 * only exercises the public declarations from include/libc64/qrand.h; no C
 * implementation is copied into the Rust crate or used at runtime.
 */
#include "libc64/qrand.h"

#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>

static uint32_t float_bits(float value) {
    union {
        float f;
        uint32_t u;
    } bits = {value};
    return bits.u;
}

static void print_u32_sequence(uint32_t seed) {
    sqrand(seed);
    printf("u %08" PRIx32, seed);
    for (unsigned i = 0; i < 16; ++i) {
        printf(" %08" PRIx32, qrand());
    }
    putchar('\n');
}

static void print_float_sequence(uint32_t seed) {
    sqrand(seed);
    printf("f %08" PRIx32, seed);
    for (unsigned i = 0; i < 12; ++i) {
        printf(" %08" PRIx32, float_bits((i & 1) ? fqrand2() : fqrand()));
    }
    putchar('\n');
}

int main(void) {
    const uint32_t seeds[] = {
        0,
        1,
        UINT32_MAX,
        UINT32_C(0x80000000),
        UINT32_C(0xfffffffe),
        UINT32_C(0x12345678),
    };
    for (unsigned i = 0; i < sizeof(seeds) / sizeof(seeds[0]); ++i) {
        print_u32_sequence(seeds[i]);
        print_float_sequence(seeds[i]);
    }
    return 0;
}
