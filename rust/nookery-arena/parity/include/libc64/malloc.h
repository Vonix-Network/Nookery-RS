#ifndef MALLOC_H
#define MALLOC_H
#include <stddef.h>
#include <stdint.h>
extern void* malloc(size_t size);
extern void free(void* ptr);
#endif
