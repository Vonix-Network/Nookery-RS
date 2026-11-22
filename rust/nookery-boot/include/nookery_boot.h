#ifndef NOOKERY_BOOT_H
#define NOOKERY_BOOT_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define NOOKERY_BOOT_MAX_CALLBACKS 16u
#define NOOKERY_BOOT_MAX_UNRESOLVED_FRAMES 16u

#define NOOKERY_BOOT_OK 0
#define NOOKERY_BOOT_SKIP 1
#define NOOKERY_BOOT_INVALID_ARGUMENT -1
#define NOOKERY_BOOT_INVALID_TABLE -2
#define NOOKERY_BOOT_CALLBACK_FAILED -3
#define NOOKERY_BOOT_INVALID_STATE -4
#define NOOKERY_BOOT_PANIC -5
#define NOOKERY_BOOT_UNRESOLVED_UNAVAILABLE -6
#define NOOKERY_BOOT_REPORT_FAILED -7

typedef int32_t (*NookeryBootCallback)(void* context);
typedef int32_t (*NookeryBootReport)(void* context, int32_t code, const char* message);

typedef struct NookeryBootStackFrame {
    uint32_t address;
    uint32_t back_chain;
    uint32_t link_register;
} NookeryBootStackFrame;

typedef int32_t (*NookeryBootUnresolvedCallback)(
    void* context,
    const NookeryBootStackFrame* frames,
    uint32_t frame_count);

typedef struct NookeryBootCallbackTable {
    void* context;
    uint8_t construct_skip;
    uint32_t constructor_count;
    NookeryBootCallback constructors[NOOKERY_BOOT_MAX_CALLBACKS];
    NookeryBootCallback main;
    uint32_t destructor_count;
    NookeryBootCallback destructors[NOOKERY_BOOT_MAX_CALLBACKS];
    NookeryBootUnresolvedCallback unresolved;
    NookeryBootReport report;
    NookeryBootCallback entry_pad_init;
    NookeryBootCallback entry_debug_mode;
    NookeryBootCallback entry_reset_register;
    NookeryBootCallback entry_mainproc;
} NookeryBootCallbackTable;

int32_t nookery_boot_register_callbacks(const NookeryBootCallbackTable* table);
int32_t nookery_boot_unregister_callbacks(void);
int32_t nookery_boot_prolog(void);
int32_t nookery_boot_entry(void);
int32_t nookery_boot_epilog(void);
int32_t nookery_boot_unresolved(
    const NookeryBootStackFrame* frames,
    uint32_t frame_count);

#ifdef __cplusplus
}
#endif

#endif
