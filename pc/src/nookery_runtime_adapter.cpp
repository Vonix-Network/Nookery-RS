#include "acgc/nookery_runtime.h"
#include "acgc/nookery_runtime_adapter.h"

#include <cstdlib>
#include <cstdio>
#include <cstring>

extern "C" {
#include "audio.h"
#include "dolphin/os.h"
#include "libforest/emu64/emu64_wrapper.h"
#include "game.h"
#include "graph.h"
#include "main.h"
#include "m_game_dlftbls.h"
extern int g_pc_running;
}

namespace {

struct GraphRoute {
    GRAPH* graph = nullptr;
    GAME* game = nullptr;
    DLFTBL_GAME* table = nullptr;
    bool draw_ok = false;
};

NookeryRuntimeCallbackStatus route_game_init(void* user_data) {
    auto* route = static_cast<GraphRoute*>(user_data);
    route->game = static_cast<GAME*>(std::malloc(route->table->alloc_size));
    if (route->game == nullptr) {
        return NOOKERY_RUNTIME_CALLBACK_FAILED;
    }
    std::memset(route->game, 0, route->table->alloc_size);
    game_class_p = route->game;
    GRAPH_SET_DOING_POINT(route->graph, GAME_CT);
    game_ct(route->game, route->table->init, route->graph);
    emu64_refresh();
    GRAPH_SET_DOING_POINT(route->graph, GAME_CT_FINISHED);
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus route_prepare_frame(void* user_data) {
    auto* route = static_cast<GraphRoute*>(user_data);
    route->game->disable_prenmi = FALSE;
    graph_setup_double_buffer(route->graph);
    route->game->disable_display = FALSE;
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus route_input(void* user_data) {
    auto* route = static_cast<GraphRoute*>(user_data);
    game_get_controller(route->game);
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus route_game_frame(
    void* user_data, double delta_seconds, uint64_t) {
    auto* route = static_cast<GraphRoute*>(user_data);
    route->graph->dt = delta_seconds;
    route->graph->dt_num_60fps_frames = delta_seconds * GRAPH_BASE_FRAME_RATE;
    route->graph->dt_total_60fps_frames += route->graph->dt_num_60fps_frames;
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus route_game_main(
    void* user_data, double, uint64_t) {
    auto* route = static_cast<GraphRoute*>(user_data);
    GRAPH_SET_DOING_POINT(route->graph, GAME_MAIN);
    game_main(route->game);
    GRAPH_SET_DOING_POINT(route->graph, GAME_MAIN_FINISHED);
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus route_finish_frame(void* user_data) {
    auto* route = static_cast<GraphRoute*>(user_data);
    GRAPH_SET_DOING_POINT(route->graph, GAME_MAIN_FINISHED);
    route->draw_ok = graph_draw_finish(route->graph) == FALSE;
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus route_submit_frame(void* user_data) {
    auto* route = static_cast<GraphRoute*>(user_data);
    if (route->draw_ok) {
        GRAPH_SET_DOING_POINT(route->graph, TASK_SET);
        graph_task_set00(route->graph);
        GRAPH_SET_DOING_POINT(route->graph, TASK_SET_FINISHED);
        route->graph->frame_counter++;
    }
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus route_audio_tick(void* user_data) {
    /* The ABI invokes this callback once per source 60 Hz audio tick. */
    auto* route = static_cast<GraphRoute*>(user_data);
    GRAPH_SET_DOING_POINT(route->graph, AUDIO);
    sAdo_GameFrame();
    GRAPH_SET_DOING_POINT(route->graph, AUDIO_FINISHED);
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus route_game_cleanup(void* user_data) {
    auto* route = static_cast<GraphRoute*>(user_data);
    if (route->game != nullptr) {
        GRAPH_SET_DOING_POINT(route->graph, GAME_18);
        GRAPH_SET_DOING_POINT(route->graph, GAME_DT);
        game_dt(route->game);
        GRAPH_SET_DOING_POINT(route->graph, GAME_DT_FINISHED);
        std::free(route->game);
        route->game = nullptr;
        game_class_p = nullptr;
    }
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbacks callbacks(GraphRoute* route) {
    NookeryRuntimeCallbacks value{};
    value.user_data = route;
    value.frame_begin = route_prepare_frame;
    value.input = route_input;
    value.draw_first = route_game_frame;
    value.update = route_game_main;
    value.audio_tick = route_audio_tick;
    value.frame_end = route_finish_frame;
    value.submit_frame = route_submit_frame;
    value.game_init = route_game_init;
    value.game_cleanup = route_game_cleanup;
    return value;
}

bool runtime_ok(NookeryRuntimeStatus status, const char* operation) {
    if (status == NOOKERY_RUNTIME_STATUS_OK) {
        return true;
    }
    std::fprintf(stderr, "[nookery-runtime] %s failed with status %d\n",
                 operation, static_cast<int>(status));
    return false;
}

} // namespace

extern "C" int acgc_nookery_runtime_graph_proc(void*) {
    GRAPH* graph = &graph_class;
    graph_ct(graph);
    DLFTBL_GAME* table = &game_dlftbls[0];

    while (table != nullptr && g_pc_running) {
        GraphRoute route{.graph = graph, .table = table};
        const NookeryRuntimeCallbacks callback_table = callbacks(&route);
        const NookeryRuntimeConfig config{SCREEN_WIDTH, SCREEN_HEIGHT, 1};
        NookeryRuntimeHandle* handle = nullptr;

        if (!runtime_ok(nookery_runtime_create(&callback_table, &config, &handle),
                        "create") ||
            !runtime_ok(nookery_runtime_boot(handle), "boot")) {
            nookery_runtime_destroy(handle);
            graph_dt(graph);
            return 1;
        }

        OSTime time = OSGetTime();
        while (route.game != nullptr && game_is_doing(route.game) && g_pc_running) {
            const OSTime current_time = OSGetTime();
            const OSTick multiplier = OSSecondsToTicks(1);
            double delta_seconds = static_cast<double>(
                static_cast<u32>(current_time - time)) /
                static_cast<double>(multiplier);
            time = current_time;
            if (!runtime_ok(nookery_runtime_frame(handle, delta_seconds), "frame")) {
                g_pc_running = 0;
                break;
            }
        }

        DLFTBL_GAME* next = nullptr;
        if (route.game != nullptr && !g_pc_running) {
            next = nullptr;
        } else if (route.game != nullptr) {
            next = static_cast<DLFTBL_GAME*>(
                graph_get_next_game_dlftbl(route.game));
        }
        runtime_ok(nookery_runtime_shutdown(handle), "shutdown");
        runtime_ok(nookery_runtime_destroy(handle), "destroy");
        table = next;
    }

    graph_dt(graph);
    return 0;
}
