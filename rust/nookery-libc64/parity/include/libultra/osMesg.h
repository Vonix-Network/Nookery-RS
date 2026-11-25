#ifndef NOOKERY_ORACLE_OSMESG_H
#define NOOKERY_ORACLE_OSMESG_H

#include "types.h"
#include "dolphin/os/OSMessage.h"

typedef void* OSMesg;

#define OS_MESG_NOBLOCK 0
#define OS_MESG_BLOCK 1

void osCreateMesgQueue(OSMessageQueue*, OSMessage*, int);
int osSendMesg(OSMessageQueue*, OSMessage, int);
int osRecvMesg(OSMessageQueue*, OSMessage*, int);

#endif
