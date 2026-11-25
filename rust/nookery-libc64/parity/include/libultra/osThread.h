#ifndef NOOKERY_ORACLE_OSTHREAD_API_H
#define NOOKERY_ORACLE_OSTHREAD_API_H

#include "types.h"
#include "dolphin/os/OSThread.h"

typedef s32 OSId;
OSId osGetThreadId(OSThread*);

#endif
