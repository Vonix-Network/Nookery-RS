#include "acgc/nookery_runtime.h"
#include "acgc/nookery_runtime_adapter.h"

#include "Famicom/famicom.h"
#include "audio.h"
#include "boot.h"
#include "dvderr.h"
#include "first_game.h"
#include "game.h"
#include "graph.h"
#include "initial_menu.h"
#include "irqmgr.h"
#include "libforest/batconfig.h"
#include "libforest/emu64/emu64_wrapper.h"
#include "libultra/libultra.h"
#include "libultra/initialize.h"
#include "dolphin/os.h"
#include "dolphin/os/OSAlarm.h"
#include "main.h"
#include "jsyswrap.h"
#include "libjsys/jsyswrapper.h"
#include "m_debug.h"
#include "m_game_dlftbls.h"
#include "m_land.h"
#include "m_time.h"
#include "padmgr.h"
#include "sys_dynamic.h"
#include "sys_stacks.h"
#include "sys_ucode.h"
#include "pc_platform.h"

#include <cstdio>
#include <cstring>
#include <cstdlib>

extern "C" {
void adjustOSArena();
void sound_initial();
void sound_initial2();
int LoadStringTable(const char* table);
void mCD_init_card();
void mMsg_aram_init2();
extern int ScreenWidth;
extern int ScreenHeight;
}
namespace {

struct RuntimeHost {
    NookeryRuntimeHandle* handle = nullptr;
    GRAPH* graph = &graph_class;
    GAME* game = nullptr;
    OSMessage serial_message{};
    OSMessage irq_messages[10]{};
    OSMessageQueue serial_queue{};
    OSMessageQueue irq_queue{};
    irqmgr_client_t irq_client{};
    bool boot_prepared = false;
};

RuntimeHost s_host;

extern "C" {

NookeryRuntimeCallbackStatus callback_ok() {
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus callback_failed() {
    return NOOKERY_RUNTIME_CALLBACK_FAILED;
}

NookeryRuntimeCallbackStatus set_screen_size(void*, uint32_t width, uint32_t height) {
    ScreenWidth = static_cast<int>(width);
    ScreenHeight = static_cast<int>(height);
    return callback_ok();
}

NookeryRuntimeCallbackStatus initial_frame_begin(void*) {
    JW_BeginFrame();
    return callback_ok();
}

NookeryRuntimeCallbackStatus initial_frame_end(void*) {
    JW_EndFrame();
    return callback_ok();
}

NookeryRuntimeCallbackStatus initialize_card(void*) {
    mCD_init_card();
    return callback_ok();
}

NookeryRuntimeCallbackStatus create_irq_manager(void* user_data) {
    auto* host = static_cast<RuntimeHost*>(user_data);
    osCreateMesgQueue(&host->serial_queue, &host->serial_message, 1);
    osCreateMesgQueue(&host->irq_queue, host->irq_messages, 10);
    CreateIRQManager(irqmgrStack + IRQMGR_STACK_SIZE, IRQMGR_STACK_SIZE, 18, 1);
    irqmgr_AddClient(&host->irq_client, &host->irq_queue);
    return callback_ok();
}

NookeryRuntimeCallbackStatus create_input_manager(void* user_data) {
    auto* host = static_cast<RuntimeHost*>(user_data);
    std::memset(padmgrStack, 0xEB, PADMGR_STACK_SIZE);
    padmgr_Init(nullptr);
    padmgr_Create(&host->serial_queue, 7, 15, padmgrStack + PADMGR_STACK_SIZE,
                  PADMGR_STACK_SIZE);
    return callback_ok();
}

NookeryRuntimeCallbackStatus runtime_frame_begin(void*) {
    JW_BeginFrame();
    return callback_ok();
}

NookeryRuntimeCallbackStatus runtime_frame_end(void*) {
    JW_EndFrame();
    return callback_ok();
}

NookeryRuntimeCallbackStatus initialize_display(void*) {
    JW_Init3();
    return callback_ok();
}

NookeryRuntimeCallbackStatus initialize_messages(void*) {
    mMsg_aram_init2();
    return callback_ok();
}

NookeryRuntimeCallbackStatus enable_land_start(void*) {
    mLd_StartFlagOn();
    return callback_ok();
}

NookeryRuntimeCallbackStatus mount_archive(void*) {
    famicom_mount_archive();
    return callback_ok();
}

NookeryRuntimeCallbackStatus dump_aram_heap(void*) {
    JC_JKRAramHeap_dump(JC_JKRAram_getAramHeap());
    return callback_ok();
}

void graph_setup_double_buffer(RuntimeHost* host) {
    std::memset(&sys_dynamic, 0, sizeof(sys_dynamic));
    sys_dynamic.start_magic = SYSDYNAMIC_START_MAGIC;
    sys_dynamic.end_magic = SYSDYNAMIC_END_MAGIC;
    THA_GA_ct(&host->graph->bg_opaque_thaga, sys_dynamic.new0, NEW0_SIZE * sizeof(Gfx));
    THA_GA_ct(&host->graph->bg_translucent_thaga, sys_dynamic.new1, NEW1_SIZE * sizeof(Gfx));
    THA_GA_ct(&host->graph->polygon_opaque_thaga, sys_dynamic.poly_opa,
              POLY_OPA_SIZE * sizeof(Gfx));
    THA_GA_ct(&host->graph->polygon_translucent_thaga, sys_dynamic.poly_xlu,
              POLY_XLU_SIZE * sizeof(Gfx));
    THA_GA_ct(&host->graph->overlay_thaga, sys_dynamic.overlay, OVERLAY_SIZE * sizeof(Gfx));
    THA_GA_ct(&host->graph->work_thaga, sys_dynamic.work, WORK_SIZE * sizeof(Gfx));
    THA_GA_ct(&host->graph->font_thaga, sys_dynamic.font, FONT_SIZE * sizeof(Gfx));
    THA_GA_ct(&host->graph->shadow_thaga, sys_dynamic.shadow, SHADOW_SIZE * sizeof(Gfx));
    THA_GA_ct(&host->graph->light_thaga, sys_dynamic.light, LIGHT_SIZE * sizeof(Gfx));
    host->graph->Gfx_list10 = sys_dynamic.new0;
    host->graph->Gfx_list11 = sys_dynamic.new1;
    host->graph->Gfx_list00 = sys_dynamic.poly_opa;
    host->graph->Gfx_list01 = sys_dynamic.poly_xlu;
    host->graph->Gfx_list04 = sys_dynamic.overlay;
    host->graph->Gfx_list05 = sys_dynamic.work;
    host->graph->Gfx_list07 = sys_dynamic.font;
    host->graph->Gfx_list08 = sys_dynamic.shadow;
    host->graph->Gfx_list09 = sys_dynamic.light;
    host->graph->gfxsave = nullptr;
}

DLFTBL_GAME* next_game_table(GAME* game) {
    auto* init = game_get_next_game_init(game);
    for (unsigned int i = 0; i < game_dlftbls_num; ++i) {
        if (game_dlftbls[i].init == init) {
            return &game_dlftbls[i];
        }
    }
    return nullptr;
}

bool create_game(RuntimeHost* host, DLFTBL_GAME* table) {
    if (table == nullptr || table->init == nullptr || table->alloc_size == 0) {
        return false;
    }
    host->game = static_cast<GAME*>(std::malloc(table->alloc_size));
    if (host->game == nullptr) {
        return false;
    }
    game_ct(host->game, table->init, host->graph);
    emu64_refresh();
    return true;
}

bool advance_game(RuntimeHost* host) {
    if (host->game == nullptr || game_is_doing(host->game)) {
        return host->game != nullptr;
    }
    DLFTBL_GAME* next = next_game_table(host->game);
    game_dt(host->game);
    std::free(host->game);
    host->game = nullptr;
    return create_game(host, next);
}

NookeryRuntimeCallbackStatus game_init(void* user_data) {
    auto* host = static_cast<RuntimeHost*>(user_data);
    graph_ct(host->graph);
    if (!create_game(host, &game_dlftbls[0])) {
        return callback_failed();
    }
    return advance_game(host) ? callback_ok() : callback_failed();
}

NookeryRuntimeCallbackStatus frame_begin(void* user_data) {
    auto* host = static_cast<RuntimeHost*>(user_data);
    if (!advance_game(host)) {
        return callback_failed();
    }
    graph_setup_double_buffer(host);
    game_get_controller(host->game);
    return callback_ok();
}

NookeryRuntimeCallbackStatus input(void*) {
    return callback_ok();
}

NookeryRuntimeCallbackStatus draw_first(void* user_data, double delta_seconds, uint64_t) {
    auto* host = static_cast<RuntimeHost*>(user_data);
    host->graph->dt = delta_seconds;
    host->graph->dt_num_60fps_frames = delta_seconds * GRAPH_BASE_FRAME_RATE;
    host->graph->dt_total_60fps_frames += host->graph->dt_num_60fps_frames;
    return callback_ok();
}

NookeryRuntimeCallbackStatus update(void*, double, uint64_t) {
    return callback_ok();
}

NookeryRuntimeCallbackStatus execute(void* user_data, double, uint64_t) {
    auto* host = static_cast<RuntimeHost*>(user_data);
    game_main(host->game);
    return callback_ok();
}

NookeryRuntimeCallbackStatus bgm(void*, double, uint64_t) {
    return callback_ok();
}

NookeryRuntimeCallbackStatus frame_end(void*) {
    return callback_ok();
}

NookeryRuntimeCallbackStatus submit_frame(void* user_data) {
    auto* host = static_cast<RuntimeHost*>(user_data);
    if (host->game->disable_display) {
        return callback_ok();
    }

    OPEN_DISP(host->graph);
    gSPBranchList(NOW_WORK_DISP++, host->graph->Gfx_list10);
    gSPBranchList(NOW_BG_OPA_DISP++, host->graph->Gfx_list08);
    gSPBranchList(NOW_SHADOW_DISP++, host->graph->Gfx_list11);
    gSPBranchList(NOW_BG_XLU_DISP++, host->graph->Gfx_list00);
    gSPBranchList(NOW_POLY_OPA_DISP++, host->graph->Gfx_list01);
    gSPBranchList(NOW_POLY_XLU_DISP++, host->graph->Gfx_list09);
    gSPBranchList(NOW_LIGHT_DISP++, host->graph->Gfx_list07);
    gSPBranchList(NOW_FONT_DISP++, host->graph->Gfx_list04);
    gDPPipeSync(NOW_OVERLAY_DISP++);
    gDPFullSync(NOW_OVERLAY_DISP++);
    gSPEndDisplayList(NOW_OVERLAY_DISP++);
    CLOSE_DISP(host->graph);

    if (THA_GA_isCrash(&host->graph->polygon_opaque_thaga) ||
        THA_GA_isCrash(&host->graph->polygon_translucent_thaga) ||
        THA_GA_isCrash(&host->graph->overlay_thaga) ||
        THA_GA_isCrash(&host->graph->font_thaga) ||
        THA_GA_isCrash(&host->graph->shadow_thaga) ||
        THA_GA_isCrash(&host->graph->light_thaga) ||
        THA_GA_isCrash(&host->graph->bg_opaque_thaga) ||
        THA_GA_isCrash(&host->graph->bg_translucent_thaga)) {
        return callback_failed();
    }

    ucode_info ucode[2]{};
    ucode[0].type = UCODE_TYPE_POLY_TEXT;
    ucode[0].ucode_p = ucode_GetPolyTextStart();
    ucode[1].type = UCODE_TYPE_SPRITE_TEXT;
    ucode[1].ucode_p = ucode_GetSpriteTextStart();
    JW_BeginFrame();
    emu64_init();
    emu64_set_ucode_info(2, ucode);
    emu64_set_first_ucode(ucode[0].ucode_p);
    emu64_taskstart(host->graph->Gfx_list05);
    emu64_cleanup();
    JW_EndFrame();
    ++host->graph->frame_counter;
    return callback_ok();
}

NookeryRuntimeCallbackStatus audio_tick(void*) {
    sAdo_GameFrame();
    return callback_ok();
}

NookeryRuntimeCallbackStatus audio_initialize(void*) {
    return callback_ok();
}

NookeryRuntimeCallbackStatus audio_cleanup(void*) {
    return callback_ok();
}

NookeryRuntimeCallbackStatus game_cleanup(void* user_data) {
    auto* host = static_cast<RuntimeHost*>(user_data);
    if (host->game != nullptr) {
        game_dt(host->game);
        std::free(host->game);
        host->game = nullptr;
    }
    graph_dt(host->graph);
    return callback_ok();
}

NookeryRuntimeCallbackStatus renderer_shutdown(void*) {
    return callback_ok();
}

NookeryRuntimeCallbackStatus platform_shutdown(void*) {
    JW_Cleanup();
    return callback_ok();
}

} // extern "C"

NookeryRuntimeCallbacks callbacks() {
    NookeryRuntimeCallbacks result{};
    result.user_data = &s_host;
    result.set_screen_size = set_screen_size;
    result.initial_frame_begin = initial_frame_begin;
    result.initial_frame_end = initial_frame_end;
    result.initialize_card = initialize_card;
    result.create_irq_manager = create_irq_manager;
    result.create_input_manager = create_input_manager;
    result.runtime_frame_begin = runtime_frame_begin;
    result.runtime_frame_end = runtime_frame_end;
    result.initialize_display = initialize_display;
    result.initialize_messages = initialize_messages;
    result.enable_land_start = enable_land_start;
    result.mount_archive = mount_archive;
    result.dump_aram_heap = dump_aram_heap;
    result.frame_begin = frame_begin;
    result.input = input;
    result.draw_first = draw_first;
    result.update = update;
    result.execute = execute;
    result.bgm = bgm;
    result.audio_tick = audio_tick;
    result.frame_end = frame_end;
    result.submit_frame = submit_frame;
    result.game_init = game_init;
    result.audio_initialize = audio_initialize;
    result.audio_cleanup = audio_cleanup;
    result.game_cleanup = game_cleanup;
    result.renderer_shutdown = renderer_shutdown;
    result.platform_shutdown = platform_shutdown;
    return result;
}

bool prepare_boot() {
    if (s_host.boot_prepared) {
        return true;
    }
    ReconfigBATs();
    InitialStartTime = osGetTime();
    OSInit();
    OSInitAlarm();
    __osInitialize_common();
    std::memset(osAppNMIBuffer, 0, 16 * sizeof(s32));
    new_Debug_mode();
    adjustOSArena();
    JW_Init();
    sound_initial();
    initial_menu_init();
    dvderr_init();
    sound_initial2();
    if (LoadStringTable("/static.str") != 0) {
        return false;
    }
    JW_Init2();
    initial_menu_cleanup();
    s_host.boot_prepared = true;
    return true;
}

} // namespace

extern "C" int acgc_nookery_runtime_start(
    uint32_t width, uint32_t height, uint8_t audio_enabled,
    NookeryRuntimeHandle** out_handle) {
    if (out_handle == nullptr || s_host.handle != nullptr || !prepare_boot()) {
        return static_cast<int>(NOOKERY_RUNTIME_STATUS_INVALID_STATE);
    }
    s_host = RuntimeHost{};
    s_host.boot_prepared = true;
    const NookeryRuntimeCallbacks host_callbacks = callbacks();
    const NookeryRuntimeConfig config{width, height, audio_enabled};
    NookeryRuntimeStatus status = nookery_runtime_create(&host_callbacks, &config, &s_host.handle);
    if (status != NOOKERY_RUNTIME_STATUS_OK) {
        return static_cast<int>(status);
    }
    status = nookery_runtime_boot(s_host.handle);
    if (status != NOOKERY_RUNTIME_STATUS_OK) {
        nookery_runtime_destroy(s_host.handle);
        s_host.handle = nullptr;
        return static_cast<int>(status);
    }
    *out_handle = s_host.handle;
    return static_cast<int>(NOOKERY_RUNTIME_STATUS_OK);
}

extern "C" int acgc_nookery_runtime_frame(NookeryRuntimeHandle* handle, double delta_seconds) {
    if (handle == nullptr || handle != s_host.handle) {
        return static_cast<int>(NOOKERY_RUNTIME_STATUS_NULL);
    }
    return static_cast<int>(nookery_runtime_frame(handle, delta_seconds));
}

extern "C" int acgc_nookery_runtime_stop(NookeryRuntimeHandle* handle) {
    if (handle == nullptr || handle != s_host.handle) {
        return static_cast<int>(NOOKERY_RUNTIME_STATUS_NULL);
    }
    const NookeryRuntimeStatus shutdown = nookery_runtime_shutdown(handle);
    const NookeryRuntimeStatus destroy = nookery_runtime_destroy(handle);
    s_host.handle = nullptr;
    if (shutdown != NOOKERY_RUNTIME_STATUS_OK) {
        return static_cast<int>(shutdown);
    }
    return static_cast<int>(destroy);
}

namespace {

struct GraphRoute {
    GRAPH* graph = nullptr;
    GAME* game = nullptr;
    DLFTBL_GAME* table = nullptr;
    bool draw_ok = false;
};

NookeryRuntimeCallbackStatus graph_route_game_init(void* user_data) {
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

NookeryRuntimeCallbackStatus graph_route_prepare_frame(void* user_data) {
    auto* route = static_cast<GraphRoute*>(user_data);
    route->game->disable_prenmi = FALSE;
    graph_setup_double_buffer(route->graph);
    route->game->disable_display = FALSE;
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus graph_route_input(void* user_data) {
    auto* route = static_cast<GraphRoute*>(user_data);
    game_get_controller(route->game);
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus graph_route_game_frame(
    void* user_data, double delta_seconds, uint64_t) {
    auto* route = static_cast<GraphRoute*>(user_data);
    route->graph->dt = delta_seconds;
    route->graph->dt_num_60fps_frames = delta_seconds * GRAPH_BASE_FRAME_RATE;
    route->graph->dt_total_60fps_frames += route->graph->dt_num_60fps_frames;
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus graph_route_game_main(
    void* user_data, double, uint64_t) {
    auto* route = static_cast<GraphRoute*>(user_data);
    GRAPH_SET_DOING_POINT(route->graph, GAME_MAIN);
    game_main(route->game);
    GRAPH_SET_DOING_POINT(route->graph, GAME_MAIN_FINISHED);
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus graph_route_finish_frame(void* user_data) {
    auto* route = static_cast<GraphRoute*>(user_data);
    GRAPH_SET_DOING_POINT(route->graph, GAME_MAIN_FINISHED);
    route->draw_ok = graph_draw_finish(route->graph) == FALSE;
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus graph_route_submit_frame(void* user_data) {
    auto* route = static_cast<GraphRoute*>(user_data);
    if (route->draw_ok) {
        GRAPH_SET_DOING_POINT(route->graph, TASK_SET);
        graph_task_set00(route->graph);
        GRAPH_SET_DOING_POINT(route->graph, TASK_SET_FINISHED);
        route->graph->frame_counter++;
    }
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus graph_route_audio_tick(void* user_data) {
    auto* route = static_cast<GraphRoute*>(user_data);
    GRAPH_SET_DOING_POINT(route->graph, AUDIO);
    sAdo_GameFrame();
    GRAPH_SET_DOING_POINT(route->graph, AUDIO_FINISHED);
    return NOOKERY_RUNTIME_CALLBACK_OK;
}

NookeryRuntimeCallbackStatus graph_route_game_cleanup(void* user_data) {
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

NookeryRuntimeCallbacks graph_route_callbacks(GraphRoute* route) {
    NookeryRuntimeCallbacks value{};
    value.user_data = route;
    value.frame_begin = graph_route_prepare_frame;
    value.input = graph_route_input;
    value.draw_first = graph_route_game_frame;
    value.update = graph_route_game_main;
    value.audio_tick = graph_route_audio_tick;
    value.frame_end = graph_route_finish_frame;
    value.submit_frame = graph_route_submit_frame;
    value.game_init = graph_route_game_init;
    value.game_cleanup = graph_route_game_cleanup;
    return value;
}

bool graph_route_runtime_ok(NookeryRuntimeStatus status, const char* operation) {
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
        const NookeryRuntimeCallbacks callback_table = graph_route_callbacks(&route);
        const NookeryRuntimeConfig config{SCREEN_WIDTH, SCREEN_HEIGHT, 1};
        NookeryRuntimeHandle* handle = nullptr;

        if (!graph_route_runtime_ok(
                nookery_runtime_create(&callback_table, &config, &handle), "create") ||
            !graph_route_runtime_ok(nookery_runtime_boot(handle), "boot")) {
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
            if (!graph_route_runtime_ok(
                    nookery_runtime_frame(handle, delta_seconds), "frame")) {
                g_pc_running = 0;
                break;
            }
        }

        DLFTBL_GAME* next = nullptr;
        if (route.game != nullptr && !g_pc_running) {
            next = nullptr;
        } else if (route.game != nullptr) {
            next = static_cast<DLFTBL_GAME*>(graph_get_next_game_dlftbl(route.game));
        }
        graph_route_runtime_ok(nookery_runtime_shutdown(handle), "shutdown");
        graph_route_runtime_ok(nookery_runtime_destroy(handle), "destroy");
        table = next;
    }

    graph_dt(graph);
    return 0;
}
