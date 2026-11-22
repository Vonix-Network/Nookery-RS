#include "acgc/nookery_runtime.h"

#include <cmath>
#include <cstdio>
#include <vector>

namespace {

struct SmokeTrace {
    std::vector<const char*> events;
    uint64_t expected_frame = 0;
};

NookeryRuntimeCallbackStatus record(void* user_data, const char* event) {
    if (user_data == nullptr) {
        return NOOKERY_RUNTIME_CALLBACK_INVALID_INPUT;
    }
    static_cast<SmokeTrace*>(user_data)->events.push_back(event);
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

#define NOOKERY_RUNTIME_VOID_CALLBACK(name, event_name)                                 \
    extern "C" NookeryRuntimeCallbackStatus name(void* user_data) {                    \
        return record(user_data, event_name);                                           \
    }

NOOKERY_RUNTIME_VOID_CALLBACK(smoke_initial_frame_begin, "initial_frame_begin")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_initial_frame_end, "initial_frame_end")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_initialize_card, "initialize_card")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_create_irq_manager, "create_irq_manager")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_create_input_manager, "create_input_manager")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_runtime_frame_begin, "runtime_frame_begin")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_runtime_frame_end, "runtime_frame_end")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_initialize_display, "initialize_display")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_initialize_messages, "initialize_messages")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_enable_land_start, "enable_land_start")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_mount_archive, "mount_archive")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_dump_aram_heap, "dump_aram_heap")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_frame_begin, "frame_begin")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_input, "input")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_frame_end, "frame_end")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_submit_frame, "submit_frame")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_audio_tick, "audio_tick")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_game_init, "game_init")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_audio_initialize, "audio_initialize")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_audio_reset, "audio_reset")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_reset, "reset")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_prenmi, "prenmi")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_game_cleanup, "game_cleanup")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_audio_cleanup, "audio_cleanup")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_renderer_shutdown, "renderer_shutdown")
NOOKERY_RUNTIME_VOID_CALLBACK(smoke_platform_shutdown, "platform_shutdown")

extern "C" NookeryRuntimeCallbackStatus smoke_set_screen_size(
    void* user_data, uint32_t width, uint32_t height) {
    if (width != 320 || height != 240) {
        return NOOKERY_RUNTIME_CALLBACK_INVALID_INPUT;
    }
    return record(user_data, "set_screen_size");
}

NookeryRuntimeCallbackStatus smoke_frame_callback(
    void* user_data, double delta_seconds, uint64_t frame_index, const char* event) {
    if (user_data == nullptr || !std::isfinite(delta_seconds) ||
        std::fabs(delta_seconds - (1.0 / 60.0)) > 1e-12 ||
        frame_index != static_cast<SmokeTrace*>(user_data)->expected_frame) {
        return NOOKERY_RUNTIME_CALLBACK_INVALID_INPUT;
    }
    return record(user_data, event);
}

#define NOOKERY_RUNTIME_FRAME_CALLBACK(name, event_name)                                \
    extern "C" NookeryRuntimeCallbackStatus name(                                     \
        void* user_data, double delta_seconds, uint64_t frame_index) {                  \
        return smoke_frame_callback(user_data, delta_seconds, frame_index, event_name);  \
    }

NOOKERY_RUNTIME_FRAME_CALLBACK(smoke_draw_first, "draw_first")
NOOKERY_RUNTIME_FRAME_CALLBACK(smoke_update, "update")
NOOKERY_RUNTIME_FRAME_CALLBACK(smoke_execute, "execute")
NOOKERY_RUNTIME_FRAME_CALLBACK(smoke_bgm, "bgm")

const char* status_name(NookeryRuntimeStatus status) {
    switch (status) {
    case NOOKERY_RUNTIME_STATUS_OK:
        return "Ok";
    case NOOKERY_RUNTIME_STATUS_NULL:
        return "Null";
    case NOOKERY_RUNTIME_STATUS_INVALID_STATE:
        return "InvalidState";
    case NOOKERY_RUNTIME_STATUS_INVALID_INPUT:
        return "InvalidInput";
    case NOOKERY_RUNTIME_STATUS_HOST_ERROR:
        return "HostError";
    case NOOKERY_RUNTIME_STATUS_CALLBACK_ERROR:
        return "CallbackError";
    case NOOKERY_RUNTIME_STATUS_PANIC:
        return "Panic";
    case NOOKERY_RUNTIME_STATUS_OVERFLOW:
        return "Overflow";
    case NOOKERY_RUNTIME_STATUS_BUSY:
        return "Busy";
    }
    return "Unknown";
}

bool expect_status(const char* operation, NookeryRuntimeStatus actual,
                   NookeryRuntimeStatus expected) {
    std::printf("[runtime-smoke] %s: %s (%d)\n", operation, status_name(actual),
                static_cast<int>(actual));
    if (actual != expected) {
        std::fprintf(stderr, "[runtime-smoke] unexpected %s status; expected %s (%d)\n",
                     operation, status_name(expected), static_cast<int>(expected));
        return false;
    }
    return true;
}

bool expect_events(const SmokeTrace& trace, const std::vector<const char*>& expected) {
    if (trace.events == expected) {
        return true;
    }
    std::fprintf(stderr, "[runtime-smoke] callback order mismatch\n  actual:");
    for (const char* event : trace.events) {
        std::fprintf(stderr, " %s", event);
    }
    std::fprintf(stderr, "\n  expected:");
    for (const char* event : expected) {
        std::fprintf(stderr, " %s", event);
    }
    std::fprintf(stderr, "\n");
    return false;
}

} // namespace

extern "C" int acgc_nookery_runtime_smoke() {
    SmokeTrace trace;
    NookeryRuntimeCallbacks callbacks{};
    callbacks.user_data = &trace;
    callbacks.set_screen_size = smoke_set_screen_size;
    callbacks.initial_frame_begin = smoke_initial_frame_begin;
    callbacks.initial_frame_end = smoke_initial_frame_end;
    callbacks.initialize_card = smoke_initialize_card;
    callbacks.create_irq_manager = smoke_create_irq_manager;
    callbacks.create_input_manager = smoke_create_input_manager;
    callbacks.runtime_frame_begin = smoke_runtime_frame_begin;
    callbacks.runtime_frame_end = smoke_runtime_frame_end;
    callbacks.initialize_display = smoke_initialize_display;
    callbacks.initialize_messages = smoke_initialize_messages;
    callbacks.enable_land_start = smoke_enable_land_start;
    callbacks.mount_archive = smoke_mount_archive;
    callbacks.dump_aram_heap = smoke_dump_aram_heap;
    callbacks.frame_begin = smoke_frame_begin;
    callbacks.input = smoke_input;
    callbacks.draw_first = smoke_draw_first;
    callbacks.update = smoke_update;
    callbacks.execute = smoke_execute;
    callbacks.bgm = smoke_bgm;
    callbacks.audio_tick = smoke_audio_tick;
    callbacks.frame_end = smoke_frame_end;
    callbacks.submit_frame = smoke_submit_frame;
    callbacks.game_init = smoke_game_init;
    callbacks.audio_initialize = smoke_audio_initialize;
    callbacks.audio_reset = smoke_audio_reset;
    callbacks.reset = smoke_reset;
    callbacks.prenmi = smoke_prenmi;
    callbacks.game_cleanup = smoke_game_cleanup;
    callbacks.audio_cleanup = smoke_audio_cleanup;
    callbacks.renderer_shutdown = smoke_renderer_shutdown;
    callbacks.platform_shutdown = smoke_platform_shutdown;

    const NookeryRuntimeConfig config{320, 240, 1};
    NookeryRuntimeHandle* handle = nullptr;
    if (!expect_status("create", nookery_runtime_create(&callbacks, &config, &handle),
                       NOOKERY_RUNTIME_STATUS_OK) ||
        handle == nullptr) {
        std::fprintf(stderr, "[runtime-smoke] create returned no handle\n");
        return 1;
    }

    const std::vector<const char*> boot_events{
        "set_screen_size", "initial_frame_begin", "initial_frame_end", "initialize_card",
        "create_irq_manager", "create_input_manager", "runtime_frame_begin",
        "runtime_frame_end", "initialize_display", "initialize_messages", "enable_land_start",
        "mount_archive", "dump_aram_heap", "game_init", "audio_initialize",
    };
    if (!expect_status("boot", nookery_runtime_boot(handle), NOOKERY_RUNTIME_STATUS_OK) ||
        !expect_events(trace, boot_events)) {
        return 1;
    }

    trace.expected_frame = 0;
    if (!expect_status("frame", nookery_runtime_frame(handle, 1.0 / 60.0),
                       NOOKERY_RUNTIME_STATUS_OK)) {
        return 1;
    }
    const std::vector<const char*> frame_events{
        "set_screen_size", "initial_frame_begin", "initial_frame_end", "initialize_card",
        "create_irq_manager", "create_input_manager", "runtime_frame_begin",
        "runtime_frame_end", "initialize_display", "initialize_messages", "enable_land_start",
        "mount_archive", "dump_aram_heap", "game_init", "audio_initialize", "frame_begin",
        "input", "draw_first", "update", "execute", "bgm", "frame_end", "submit_frame",
        "audio_tick",
    };
    if (!expect_events(trace, frame_events)) {
        return 1;
    }

    if (!expect_status("shutdown", nookery_runtime_shutdown(handle), NOOKERY_RUNTIME_STATUS_OK) ||
        !expect_status("destroy", nookery_runtime_destroy(handle), NOOKERY_RUNTIME_STATUS_OK)) {
        return 1;
    }
    std::printf("[runtime-smoke] callback order and lifecycle passed\n");
    return 0;
}
