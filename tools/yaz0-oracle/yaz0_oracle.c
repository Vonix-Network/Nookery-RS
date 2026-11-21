/*
 * Differential oracle harness for Yaz0 decoding.
 *
 * The functions below are copied VERBATIM from the C implementation that the
 * port must match, so this harness is the reference rather than a paraphrase:
 *
 *   aurora_yaz0_decode  <- pc/src/pc_assets.c:47-94
 *   pc_load_be32        <- pc/include/pc_bswap.h:24
 *
 * Only the allocation calls are unchanged; `free` on failure paths is kept
 * exactly as upstream does it.  The harness reads a Yaz0 file, decodes it,
 * prints the decoded size and an FNV-1a 64-bit digest of the output, and on
 * failure prints the precise rejection reason so Rust can be compared
 * failure-for-failure, not just success-for-success.
 *
 * Build:  cc -O1 -std=c11 -Wall -Wextra -Werror -o yaz0_oracle yaz0_oracle.c
 * Usage:  ./yaz0_oracle <file.szs>
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t u8;
typedef uint32_t u32;

/* pc/include/pc_bswap.h:24 */
static inline unsigned int pc_load_be32(const void* src) {
    const unsigned char* p = (const unsigned char*)src;
    return ((unsigned int)p[0] << 24) | ((unsigned int)p[1] << 16) |
           ((unsigned int)p[2] << 8) | (unsigned int)p[3];
}

/* Why a decode failed, so the Rust port can be compared reason-for-reason. */
enum yaz0_result {
    YAZ0_OK = 0,
    YAZ0_TOO_SHORT,      /* src_size < 16 */
    YAZ0_BAD_MAGIC,      /* memcmp(src, "Yaz0", 4) != 0 */
    YAZ0_ALLOC_FAILED,
    YAZ0_BAD_DISTANCE,   /* distance >= dst_pos */
    YAZ0_SHORT_OUTPUT    /* dst_pos != decoded_size */
};

static const char* yaz0_result_name(enum yaz0_result r) {
    switch (r) {
        case YAZ0_OK: return "OK";
        case YAZ0_TOO_SHORT: return "TOO_SHORT";
        case YAZ0_BAD_MAGIC: return "BAD_MAGIC";
        case YAZ0_ALLOC_FAILED: return "ALLOC_FAILED";
        case YAZ0_BAD_DISTANCE: return "BAD_DISTANCE";
        case YAZ0_SHORT_OUTPUT: return "SHORT_OUTPUT";
        default: return "UNKNOWN";
    }
}

/* VERBATIM from pc/src/pc_assets.c:47-94, with only the failure reason recorded. */
static u8* aurora_yaz0_decode_ex(const u8* src, u32 src_size, u32* out_size,
                                 enum yaz0_result* why) {
    u32 decoded_size, src_pos = 16, dst_pos = 0;
    u8* dst;
    int bit;

    *why = YAZ0_OK;
    if (src_size < 16 || memcmp(src, "Yaz0", 4) != 0) {
        *why = (src_size < 16) ? YAZ0_TOO_SHORT : YAZ0_BAD_MAGIC;
        return NULL;
    }
    decoded_size = pc_load_be32(src + 4);
    dst = (u8*)malloc(decoded_size);
    if (dst == NULL) {
        *why = YAZ0_ALLOC_FAILED;
        return NULL;
    }

    while (dst_pos < decoded_size && src_pos < src_size) {
        u8 flags = src[src_pos++];
        for (bit = 7; bit >= 0 && dst_pos < decoded_size; --bit) {
            if (src_pos >= src_size) break;
            if (flags & (1 << bit)) {
                dst[dst_pos++] = src[src_pos++];
            } else {
                u8 b1, b2;
                u32 distance, length, copy_pos;
                if (src_pos + 1 >= src_size) break;
                b1 = src[src_pos++];
                b2 = src[src_pos++];
                distance = ((u32)(b1 & 0x0F) << 8) | b2;
                if ((b1 >> 4) == 0) {
                    if (src_pos >= src_size) break;
                    length = (u32)src[src_pos++] + 0x12;
                } else {
                    length = (u32)(b1 >> 4) + 2;
                }
                if (distance >= dst_pos) {
                    free(dst);
                    *why = YAZ0_BAD_DISTANCE;
                    return NULL;
                }
                copy_pos = dst_pos - distance - 1;
                while (length-- != 0 && dst_pos < decoded_size) {
                    dst[dst_pos++] = dst[copy_pos++];
                }
            }
        }
    }

    if (dst_pos != decoded_size) {
        free(dst);
        *why = YAZ0_SHORT_OUTPUT;
        return NULL;
    }
    *out_size = decoded_size;
    return dst;
}

/* FNV-1a 64-bit: a stable digest the Rust port must reproduce exactly. */
static uint64_t fnv1a64(const u8* data, size_t size) {
    uint64_t hash = 0xcbf29ce484222325ull;
    size_t i;
    for (i = 0; i < size; ++i) {
        hash ^= (uint64_t)data[i];
        hash *= 0x100000001b3ull;
    }
    return hash;
}

static u8* read_file(const char* path, size_t* out_size) {
    FILE* file = fopen(path, "rb");
    long length;
    u8* buffer;
    size_t read;

    if (file == NULL) return NULL;
    if (fseek(file, 0, SEEK_END) != 0 || (length = ftell(file)) < 0 ||
        fseek(file, 0, SEEK_SET) != 0) {
        fclose(file);
        return NULL;
    }
    buffer = (u8*)malloc((size_t)length);
    if (buffer == NULL) {
        fclose(file);
        return NULL;
    }
    read = fread(buffer, 1, (size_t)length, file);
    fclose(file);
    if (read != (size_t)length) {
        free(buffer);
        return NULL;
    }
    *out_size = (size_t)length;
    return buffer;
}

int main(int argc, char** argv) {
    u8* src;
    size_t src_size = 0;
    u32 out_size = 0;
    enum yaz0_result why = YAZ0_OK;
    u8* out;

    if (argc != 2) {
        fprintf(stderr, "usage: %s <file.szs>\n", argv[0]);
        return 2;
    }
    src = read_file(argv[1], &src_size);
    if (src == NULL) {
        fprintf(stderr, "cannot read %s\n", argv[1]);
        return 2;
    }

    out = aurora_yaz0_decode_ex(src, (u32)src_size, &out_size, &why);
    printf("input_size=%zu\n", src_size);
    printf("result=%s\n", yaz0_result_name(why));
    if (out != NULL) {
        printf("decoded_size=%u\n", out_size);
        printf("decoded_fnv1a64=%016llx\n", (unsigned long long)fnv1a64(out, out_size));
        printf("decoded_first16=");
        for (u32 i = 0; i < 16 && i < out_size; ++i) printf("%02x", out[i]);
        printf("\n");
        free(out);
    } else {
        printf("decoded_size=0\n");
        printf("decoded_fnv1a64=0000000000000000\n");
        printf("decoded_first16=\n");
    }
    free(src);
    return 0;
}
