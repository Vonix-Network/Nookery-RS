/*
 * Differential oracle for the portable actor seams.
 *
 * Keep the 32-bit shim first in the compile command.  The game's shipped
 * include/types.h maps u32 through unsigned long, which is 64-bit on the host.
 */
#include "types.h"

#include <math.h>
#include <stdint.h>
#include <stdio.h>

enum status { FREE_FLY = 0, FALL_FLY = 1, FALL_FLY2 = 2, SOMER_FLY = 3, STOP_FLY = 5 };

static float rubber_move(float now, float target, float step, float dt) {
    return now + (1.0f - powf(1.0f - step, dt)) * (target - now);
}

static int select_fly(int stick_y, float draw) {
    int mode = ((int)(draw * 7.0f)) & 1;
    if (stick_y > 67) return mode != 0 ? FALL_FLY2 : SOMER_FLY;
    return FREE_FLY;
}

static float bg_offset(int status) {
    return status == FALL_FLY || status == STOP_FLY ? -5.5f : 0.0f;
}

static void airplane_case(float speed, float rot_x, float rot_z, float dt, int grounded, int wall,
                          int *status, int *dust) {
    float accum = 0.0f;
    int ground_timer = 0;
    speed = rubber_move(speed, 3.0f, 0.01f, dt);
    rot_x = rubber_move(rot_x, 19.0f, 0.045f, dt);
    rot_z = rubber_move(rot_z, 0.0f, 0.05f, dt);
    if (grounded) {
        speed = rubber_move(speed, 0.0f, 0.6f, dt);
        if (speed < 1.0f) {
            speed = 0.0f;
            *status = STOP_FLY;
            accum += dt;
            int ticks = (int)accum;
            accum -= (float)ticks;
            if (ticks > 4) ticks = 4;
            for (int i = 0; i < ticks; ++i) {
                if (ground_timer >= 5) {
                    ground_timer = 0;
                    ++*dust;
                }
                ++ground_timer;
            }
        }
    }
    if (wall) *status = FALL_FLY;
    printf("air %.9g %.9g %.9g %d %d %.9g\n", speed, rot_x, rot_z, *status, *dust,
           bg_offset(*status));
}

static void insect_case(int type, float patience, float life, float alpha_time, int alpha0, int alpha1,
                        int alpha2, uint8_t flags, float stress, float dt) {
    if (fabsf(stress) < 0.008f) {
        patience -= 0.5f * dt;
        if (patience < 0.0f) patience = 0.0f;
    } else {
        patience += stress * 0.5f;
        if (patience > 100.0f) patience = 100.0f;
    }
    life -= dt;
    if (life <= 0.0f) {
        flags |= (uint8_t)(1 << 3);
        life = 0.0f;
    }
    if (life == 0.0f && alpha_time > 0.0f) {
        alpha_time -= dt;
        float fade = 24.0f - alpha_time;
        if (fade > 0.0f) {
            if (type == 27) {
                int alpha = (int)(fade * 11.0f);
                if (alpha >= 255) {
                    alpha = 255; alpha0 = 0; alpha1 = 0; flags |= 1;
                }
                alpha2 = alpha;
            } else {
                int alpha = 255 - (int)(fade * 11.0f);
                if (alpha <= 0) { alpha = 0; flags |= 1; }
                alpha0 = alpha;
            }
        }
    }
    printf("ins %.9g %.9g %.9g %d %d %d %u\n", patience, life, alpha_time, alpha0, alpha1, alpha2,
           (unsigned)flags);
}

int main(void) {
    int status = FREE_FLY;
    int dust = 0;
    airplane_case(0.5f, 0.0f, 2.0f, 1.0f, 1, 1, &status, &dust);
    printf("select %d %d %d\n", select_fly(67, 0.99f), select_fly(68, 0.0f), select_fly(68, 0.5f));
    insect_case(0, 10.0f, 0.5f, 24.0f, 255, 0, 0, 0, 0.0f, 1.0f);
    insect_case(27, 0.0f, 0.0f, 1.0f, 255, 255, 0, 0, 0.0f, 1.0f);
    printf("offset %.9g %.9g\n", bg_offset(FALL_FLY), bg_offset(FREE_FLY));
    return 0;
}
