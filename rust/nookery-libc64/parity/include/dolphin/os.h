#ifndef NOOKERY_ORACLE_DOLPHIN_OS_H
#define NOOKERY_ORACLE_DOLPHIN_OS_H

#include "types.h"
#include "dolphin/os/OSMessage.h"
#include "dolphin/os/OSTime.h"

void OSReport(const char*, ...);
void OSPanic(const char*, int, const char*, ...);
OSTime osGetTime(void);
int osGetThreadId(OSThread*);
void osCreateMesgQueue(OSMessageQueue*, OSMessage*, int);
int osSendMesg(OSMessageQueue*, OSMessage, int);
int osRecvMesg(OSMessageQueue*, OSMessage*, int);

#endif
