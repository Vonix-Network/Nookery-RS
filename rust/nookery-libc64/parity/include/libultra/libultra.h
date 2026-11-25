#ifndef NOOKERY_ORACLE_LIBULTRA_H
#define NOOKERY_ORACLE_LIBULTRA_H

#include "types.h"

#define ALIGN_PREV(u, align) ((u) & ~((align) - 1))
#define ALIGN_NEXT(u, align) (((u) + ((align) - 1)) & ~((align) - 1))
#define OSDiffTick(tick1, tick0) ((s32)((tick1) - (tick0)))
#define OSTicksToMicroseconds(ticks) (ticks)

void OSReport(const char*, ...);
void OSPanic(const char*, int, const char*, ...);

#endif
