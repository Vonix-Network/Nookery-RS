#ifndef NOOKERY_ORACLE_OSMESSAGE_H
#define NOOKERY_ORACLE_OSMESSAGE_H

#include "dolphin/os/OSThread.h"

typedef void* OSMessage;
typedef struct OSMessageQueue {
    OSThreadQueue queueSend;
    OSThreadQueue queueReceive;
    OSMessage* msgArray;
    int msgCount;
    int firstIndex;
    int usedCount;
} OSMessageQueue;

#endif
