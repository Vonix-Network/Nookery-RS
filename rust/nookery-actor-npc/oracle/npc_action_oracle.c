/*
 * Test-only C oracle for the request/action boundary.
 *
 * The three static functions below are copied verbatim from
 * src/actor/npc/ac_npc_action.c_inc:445-498. The small target-layout prelude
 * and callback table make that source block executable without the game's
 * global runtime. tests/differential.rs checks the verbatim block before it
 * compiles this file.
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t u8;
typedef uint16_t u16;
typedef uint32_t u32;
typedef int16_t s16;
typedef float f32;
typedef u16 mActor_name_t;
typedef struct game_play_s GAME_PLAY;
struct game_play_s {
    int unused;
};

#define FALSE 0
#define TRUE 1
#define mem_copy(dst, src, size) memcpy((dst), (src), (size))
#define aNPC_ACTION_END_STEP 0xFF
#define aNPC_ACT_NONE 0xFF
#define aNPC_ACTION_PROC_TYPE_INIT 0
#define aNPC_ACTION_PROC_TYPE_CHG_DATA 1
#define aNPC_ACTION_PROC_TYPE_MAIN 2

typedef struct npc_action_s {
    u8 priority;
    u8 idx;
    u8 step;
    u8 type;
    u8 prev_idx;
    u8 prev_step;
    f32 act_timer;
    u8 feel;
    u8 act_obj;
    u16 act_obj_id;
    s16 move_x;
    s16 move_z;
    u32 act_proc;
} aNPC_action_c;

typedef struct npc_request_s {
    u8 act_priority;
    u8 act_idx;
    u8 act_type;
    u16 act_args[6];
    u8 umb_flag;
    u8 _11;
    u8 head_priority;
    u8 head_type;
    u32 head_target;
    f32 head_pos[3];
} aNPC_request_c;

typedef struct npc_actor_s {
    aNPC_action_c action;
    aNPC_request_c request;
} NPC_ACTOR;

typedef void (*aNPC_ACT_PROC)(NPC_ACTOR*, GAME_PLAY*, int);

static char events[16][32];
static int event_count;

static void record_event(const char* kind, u8 idx, int phase) {
    (void)snprintf(events[event_count], sizeof(events[event_count]), "%s:%u:%d", kind, idx, phase);
    event_count++;
}

static void aNPC_set_act_obj(NPC_ACTOR* nactorx, GAME_PLAY* play) {
    (void)nactorx;
    (void)play;
    record_event("R", 0, 0);
}

static void oracle_action_proc(NPC_ACTOR* nactorx, GAME_PLAY* play, int phase) {
    (void)play;
    record_event("D", nactorx->action.idx, phase);
    if (phase == aNPC_ACTION_PROC_TYPE_CHG_DATA) {
        nactorx->action.act_obj = 7;
        nactorx->action.act_obj_id = 0x1234;
    }
}

static aNPC_ACT_PROC aNPC_act_proc[] = {
    &oracle_action_proc, &oracle_action_proc, &oracle_action_proc, &oracle_action_proc,
    &oracle_action_proc, &oracle_action_proc, &oracle_action_proc, &oracle_action_proc,
    &oracle_action_proc, &oracle_action_proc, &oracle_action_proc, &oracle_action_proc,
    &oracle_action_proc, &oracle_action_proc, &oracle_action_proc, &oracle_action_proc,
    &oracle_action_proc, &oracle_action_proc, &oracle_action_proc, &oracle_action_proc,
    &oracle_action_proc, &oracle_action_proc, &oracle_action_proc,
};

static void aNPC_chk_request_act(NPC_ACTOR* nactorx, GAME_PLAY* play) {
    if (nactorx->request.act_priority >= nactorx->action.priority && nactorx->request.act_priority != 0) {
        u8 prev_idx = nactorx->action.idx;
        u8 prev_obj = nactorx->action.act_obj;
        mActor_name_t prev_obj_name = nactorx->action.act_obj_id;

        nactorx->action.prev_idx = nactorx->action.idx;
        nactorx->action.prev_step = nactorx->action.step;
        nactorx->action.priority = nactorx->request.act_priority;
        nactorx->action.idx = nactorx->request.act_idx;
        nactorx->action.type = nactorx->request.act_type;

        (*aNPC_act_proc[nactorx->action.idx])(nactorx, play, aNPC_ACTION_PROC_TYPE_CHG_DATA);
        if (nactorx->action.act_obj != prev_obj || nactorx->action.act_obj_id != prev_obj_name) {
            aNPC_set_act_obj(nactorx, play);
        }

        if (nactorx->request.act_idx != prev_idx) {
            (*aNPC_act_proc[nactorx->action.idx])(nactorx, play, aNPC_ACTION_PROC_TYPE_INIT);
        }
    }

    nactorx->request.act_priority = 0;
}

static int aNPC_set_request_act(NPC_ACTOR* nactorx, u8 prio, u8 act_idx, u8 act_type, u16* args) {
    int ret = FALSE;

    if (prio >= nactorx->request.act_priority) {
        nactorx->request.act_priority = prio;
        nactorx->request.act_idx = act_idx;
        nactorx->request.act_type = act_type;

        if (args != NULL) {
            mem_copy((u8*)nactorx->request.act_args, (u8*)args, sizeof(nactorx->request.act_args));
        }

        ret = TRUE;
    }

    return ret;
}

static void aNPC_action_proc(NPC_ACTOR* nactorx, GAME_PLAY* play) {
    aNPC_chk_request_act(nactorx, play);

    if (nactorx->action.step != aNPC_ACTION_END_STEP && nactorx->action.idx != aNPC_ACT_NONE) {
        (*aNPC_act_proc[nactorx->action.idx])(nactorx, play, aNPC_ACTION_PROC_TYPE_MAIN);
    }

    if (nactorx->action.step == aNPC_ACTION_END_STEP) {
        nactorx->action.priority = 0;
    }
}

static unsigned parse(const char* text) {
    return (unsigned)strtoul(text, NULL, 0);
}

int main(int argc, char** argv) {
    NPC_ACTOR actor = {0};
    GAME_PLAY play = {0};
    u16 args[6] = {11, 22, 33, 44, 55, 66};
    int accepted;

    if (argc != 9) {
        return 2;
    }

    actor.action.priority = (u8)parse(argv[1]);
    actor.action.idx = (u8)parse(argv[2]);
    actor.action.step = (u8)parse(argv[3]);
    actor.action.act_obj = (u8)parse(argv[4]);
    actor.action.act_obj_id = (u16)parse(argv[5]);
    actor.request.act_priority = (u8)parse(argv[6]);
    actor.request.act_idx = (u8)parse(argv[7]);
    actor.request.act_type = (u8)parse(argv[8]);

    accepted = aNPC_set_request_act(&actor, actor.request.act_priority, actor.request.act_idx,
                                    actor.request.act_type, args);
    aNPC_action_proc(&actor, &play);

    printf("accepted=%d action=%u,%u,%u,%u,%u,%u request=%u,%u,%u args=%u,%u,%u,%u,%u,%u events=",
           accepted, actor.action.priority, actor.action.idx, actor.action.step, actor.action.type,
           actor.action.prev_idx, actor.action.prev_step, actor.request.act_priority,
           actor.request.act_idx, actor.request.act_type, actor.request.act_args[0],
           actor.request.act_args[1], actor.request.act_args[2], actor.request.act_args[3],
           actor.request.act_args[4], actor.request.act_args[5]);
    for (int i = 0; i < event_count; i++) {
        printf("%s%s", i == 0 ? "" : ";", events[i]);
    }
    putchar('\n');
    return 0;
}
