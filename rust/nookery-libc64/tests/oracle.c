#include <stdint.h>
#include <stdio.h>
#include <stddef.h>
#include <string.h>

#include "libc64/math64.h"
#include "libc64/sleep.h"
#include "libultra/os_timer.h"

static void print_f32(const char *name, uint32_t input, float value) {
    uint32_t output;
    memcpy(&output, &value, sizeof output);
    printf("%s %08x %08x\n", name, input, output);
}

void osCreateMesgQueue(OSMessageQueue *mq, OSMessage msg, int msg_count) {
    (void)mq;
    printf("sleep.create %d %d\n", msg != NULL, msg_count);
}

int osSetTimer(OSTimer *timer, OSTime countdown, OSTime interval,
               OSMessageQueue *mq, OSMessage msg) {
    (void)timer;
    (void)mq;
    printf("sleep.set %lld %lld %d\n", (long long)countdown,
           (long long)interval, msg == NULL);
    return 0;
}

int osRecvMesg(OSMessageQueue *mq, OSMessage *msg, int flags) {
    (void)mq;
    printf("sleep.recv %d %d\n", msg == NULL, flags);
    return 0;
}

int main(void) {
    printf("layout.queue %zu %zu %zu %zu %zu %zu %zu\n",
           sizeof(OSMessageQueue), offsetof(OSMessageQueue, queueSend),
           offsetof(OSMessageQueue, queueReceive),
           offsetof(OSMessageQueue, msgArray), offsetof(OSMessageQueue, msgCount),
           offsetof(OSMessageQueue, firstIndex), offsetof(OSMessageQueue, usedCount));
    printf("layout.alarm %zu %zu %zu %zu %zu %zu %zu\n",
           sizeof(OSAlarm), offsetof(OSAlarm, handler), offsetof(OSAlarm, fire),
           offsetof(OSAlarm, prev), offsetof(OSAlarm, next),
           offsetof(OSAlarm, period), offsetof(OSAlarm, start));
    printf("layout.timer %zu %zu %zu %zu %zu %zu %zu %zu\n",
           sizeof(OSTimer), offsetof(OSTimer, alarm), offsetof(OSTimer, next),
           offsetof(OSTimer, prev), offsetof(OSTimer, interval),
           offsetof(OSTimer, value), offsetof(OSTimer, mq),
           offsetof(OSTimer, msg));

    static const uint32_t values[] = {
        0x00000000u, 0x80000000u, 0x3f800000u, 0xbf800000u,
        0x40000000u, 0xc0000000u, 0x7f800000u,
    };
    union {
        uint32_t bits;
        float value;
    } x, y;

    for (size_t i = 0; i < sizeof values / sizeof values[0]; i++) {
        x.bits = values[i];
        print_f32("fsqrt", x.bits, libc64_fsqrt(x.value));
        print_f32("facos", x.bits, facos(x.value));
        for (size_t j = 0; j < sizeof values / sizeof values[0]; j++) {
            y.bits = values[j];
            print_f32("fatan2", x.bits ^ y.bits, fatan2(x.value, y.value));
        }
    }

    const OSTime countdowns[] = {-1, 0, 1, 7, INT64_MAX};
    for (size_t i = 0; i < sizeof countdowns / sizeof countdowns[0]; i++) {
        csleep(countdowns[i]);
    }
    return 0;
}
