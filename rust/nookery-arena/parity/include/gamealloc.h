#ifndef GAMEALLOC_H
#define GAMEALLOC_H
#include <stddef.h>
#include <stdint.h>
typedef uint32_t u32;
typedef struct gameAllocList_s {
  struct gameAllocList_s* next;
  struct gameAllocList_s* prev;
  size_t alloc_size;
  u32 pad;
} GameAllocList;
typedef struct gameAlloc_s {
  GameAllocList head;
  GameAllocList* tail;
} GameAlloc;
extern void* gamealloc_malloc(GameAlloc* gamealloc, size_t size);
extern void gamealloc_free(GameAlloc* gamealloc, void* ptr);
extern void gamealloc_cleanup(GameAlloc* gamealloc);
extern void gamealloc_init(GameAlloc* gamealloc);
#endif
