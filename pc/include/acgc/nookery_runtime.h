/*
 * Narrow C ABI for the opt-in nookery-runtime host seam.
 *
 * This header intentionally contains only fixed-width C values, function
 * pointers, and an opaque handle.  It exposes no Rust references, slices,
 * ownership-bearing Rust types, or Rust unwinding ABI.  The Rust release
 * profile retains unwind support so its exported functions can catch callback
 * panics and return NOOKERY_RUNTIME_STATUS_PANIC; C/C++ callbacks must return
 * a status and must not throw across this boundary.
 */
#ifndef ACGC_NOOKERY_RUNTIME_H
#define ACGC_NOOKERY_RUNTIME_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum NookeryRuntimeStatus {
    NOOKERY_RUNTIME_STATUS_OK = 0,
    NOOKERY_RUNTIME_STATUS_NULL = 1,
    NOOKERY_RUNTIME_STATUS_INVALID_STATE = 2,
    NOOKERY_RUNTIME_STATUS_INVALID_INPUT = 3,
    NOOKERY_RUNTIME_STATUS_HOST_ERROR = 4,
    NOOKERY_RUNTIME_STATUS_CALLBACK_ERROR = 5,
    NOOKERY_RUNTIME_STATUS_PANIC = 6,
    NOOKERY_RUNTIME_STATUS_OVERFLOW = 7,
    NOOKERY_RUNTIME_STATUS_BUSY = 8,
} NookeryRuntimeStatus;

typedef enum NookeryRuntimeCallbackStatus {
    NOOKERY_RUNTIME_CALLBACK_OK = 0,
    NOOKERY_RUNTIME_CALLBACK_FAILED = 1,
    NOOKERY_RUNTIME_CALLBACK_UNAVAILABLE = 2,
    NOOKERY_RUNTIME_CALLBACK_INVALID_INPUT = 3,
} NookeryRuntimeCallbackStatus;

typedef struct NookeryRuntimeHandle NookeryRuntimeHandle;

typedef NookeryRuntimeCallbackStatus (*NookeryRuntimeVoidCallback)(void* user_data);
typedef NookeryRuntimeCallbackStatus (*NookeryRuntimeScreenCallback)(
    void* user_data, uint32_t width, uint32_t height);
typedef NookeryRuntimeCallbackStatus (*NookeryRuntimeFrameCallback)(
    void* user_data, double delta_seconds, uint64_t frame_index);
typedef NookeryRuntimeCallbackStatus (*NookeryRuntimeResizeCallback)(
    void* user_data, int64_t requested_size, size_t* actual_size);

typedef struct NookeryRuntimeCallbacks {
    void* user_data;
    NookeryRuntimeScreenCallback set_screen_size;
    NookeryRuntimeVoidCallback initial_frame_begin;
    NookeryRuntimeVoidCallback initial_frame_end;
    NookeryRuntimeVoidCallback initialize_card;
    NookeryRuntimeVoidCallback create_irq_manager;
    NookeryRuntimeVoidCallback create_input_manager;
    NookeryRuntimeVoidCallback runtime_frame_begin;
    NookeryRuntimeVoidCallback runtime_frame_end;
    NookeryRuntimeVoidCallback initialize_display;
    NookeryRuntimeVoidCallback initialize_messages;
    NookeryRuntimeVoidCallback enable_land_start;
    NookeryRuntimeVoidCallback mount_archive;
    NookeryRuntimeVoidCallback dump_aram_heap;
    NookeryRuntimeVoidCallback frame_begin;
    NookeryRuntimeVoidCallback input;
    NookeryRuntimeFrameCallback draw_first;
    NookeryRuntimeFrameCallback update;
    NookeryRuntimeFrameCallback execute;
    NookeryRuntimeFrameCallback bgm;
    NookeryRuntimeVoidCallback audio_tick;
    NookeryRuntimeVoidCallback frame_end;
    NookeryRuntimeVoidCallback submit_frame;
    NookeryRuntimeVoidCallback game_init;
    NookeryRuntimeVoidCallback audio_initialize;
    NookeryRuntimeVoidCallback audio_reset;
    NookeryRuntimeVoidCallback reset;
    NookeryRuntimeVoidCallback prenmi;
    NookeryRuntimeResizeCallback resize_game_memory;
    NookeryRuntimeVoidCallback game_cleanup;
    NookeryRuntimeVoidCallback audio_cleanup;
    NookeryRuntimeVoidCallback renderer_shutdown;
    NookeryRuntimeVoidCallback platform_shutdown;
} NookeryRuntimeCallbacks;

typedef struct NookeryRuntimeConfig {
    uint32_t width;
    uint32_t height;
    uint8_t audio_enabled;
} NookeryRuntimeConfig;

NookeryRuntimeStatus nookery_runtime_create(
    const NookeryRuntimeCallbacks* callbacks,
    const NookeryRuntimeConfig* config,
    NookeryRuntimeHandle** out_handle);
NookeryRuntimeStatus nookery_runtime_boot(NookeryRuntimeHandle* handle);
NookeryRuntimeStatus nookery_runtime_frame(
    NookeryRuntimeHandle* handle, double delta_seconds);
NookeryRuntimeStatus nookery_runtime_reset(NookeryRuntimeHandle* handle);
NookeryRuntimeStatus nookery_runtime_prenmi(NookeryRuntimeHandle* handle);
NookeryRuntimeStatus nookery_runtime_resize(
    NookeryRuntimeHandle* handle, int64_t requested_size, size_t* actual_size);
NookeryRuntimeStatus nookery_runtime_shutdown(NookeryRuntimeHandle* handle);
NookeryRuntimeStatus nookery_runtime_destroy(NookeryRuntimeHandle* handle);

#ifdef __cplusplus
}
#endif

#endif /* ACGC_NOOKERY_RUNTIME_H */
