#ifndef NOOKERY_LIBC64_ORACLE_TYPES_H
#define NOOKERY_LIBC64_ORACLE_TYPES_H

/*
 * The shipped source includes the game's broad types.h, whose target typedefs
 * intentionally use 32-bit `unsigned long`.  The host compiler's `unsigned
 * long` is 64-bit, so this test-only shim supplies the same ABI widths while
 * leaving libc64/qrand.h and qrand.c themselves untouched.
 */
#include <stdint.h>

typedef int8_t s8;
typedef int16_t s16;
typedef int32_t s32;
typedef int64_t s64;
typedef uint8_t u8;
typedef uint16_t u16;
typedef uint32_t u32;
typedef uint64_t u64;
typedef float f32;
typedef double f64;
typedef int BOOL;

#endif
