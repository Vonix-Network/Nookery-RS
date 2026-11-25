#ifndef NOOKERY_ORACLE_OSTHREAD_H
#define NOOKERY_ORACLE_OSTHREAD_H

typedef struct OSThread OSThread;
typedef struct OSThreadQueue {
    OSThread* head;
    OSThread* tail;
} OSThreadQueue;

#endif
