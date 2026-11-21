#ifndef NOOKERY_CONFIG_H
#define NOOKERY_CONFIG_H

#include <stddef.h>
#include <stdint.h>

/*
 * PCSettings is owned by the existing PC implementation. Consumers must make
 * pc/include available on the include path (for example, -I../../pc/include
 * from tools/settings-abi); this header deliberately reuses that definition.
 */
#include "pc_settings.h"

#ifdef __cplusplus
extern "C" {
#endif

#define NOOKERY_SETTINGS_ABI_VERSION 1u
#define NOOKERY_SETTINGS_STATUS_OK 0u
#define NOOKERY_SETTINGS_STATUS_FILE_MISSING 1u
#define NOOKERY_SETTINGS_STATUS_EMPTY_FILE 2u
#define NOOKERY_SETTINGS_STATUS_MALFORMED_JSON 3u
#define NOOKERY_SETTINGS_STATUS_WRONG_ROOT_TYPE 4u
#define NOOKERY_SETTINGS_STATUS_INVALID_ARGUMENT 5u
#define NOOKERY_SETTINGS_STATUS_IO_ERROR 6u
#define NOOKERY_SETTINGS_STATUS_SAVE_ERROR 7u
#define NOOKERY_SETTINGS_STATUS_WRONG_TYPE 8u
#define NOOKERY_SETTINGS_STATUS_INVALID_DIRECTION 9u
#define NOOKERY_SETTINGS_STATUS_MAX 9u

typedef struct NookerySettingsHandle NookerySettingsHandle;

uint32_t nookery_settings_abi_version(void);
uint32_t nookery_settings_status_count(void);
const char *nookery_settings_status_name(uint32_t status);

uint32_t nookery_settings_create(const char *path, NookerySettingsHandle **out_handle);
void nookery_settings_destroy(NookerySettingsHandle *handle);
uint32_t nookery_settings_load(NookerySettingsHandle *handle, PCSettings *out_settings);
uint32_t nookery_settings_save(NookerySettingsHandle *handle, const PCSettings *settings);
uint32_t nookery_settings_apply(PCSettings *settings);
uint32_t nookery_settings_cycle_resolution(int *width, int *height, int direction);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* NOOKERY_CONFIG_H */
