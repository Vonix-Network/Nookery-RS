#ifndef ACGC_NOOKERY_RUNTIME_ADAPTER_H
#define ACGC_NOOKERY_RUNTIME_ADAPTER_H

#include "acgc/nookery_runtime.h"

#ifdef __cplusplus
extern "C" {
#endif

/* The production PC host owns this opaque runtime through these helpers. */
int acgc_nookery_runtime_start(
    uint32_t width, uint32_t height, uint8_t audio_enabled,
    NookeryRuntimeHandle** out_handle);
int acgc_nookery_runtime_frame(NookeryRuntimeHandle* handle, double delta_seconds);
int acgc_nookery_runtime_stop(NookeryRuntimeHandle* handle);

/* Optional callback-order smoke; it is not part of the production route. */
int acgc_nookery_runtime_smoke(void);

#ifdef __cplusplus
}
#endif

#endif /* ACGC_NOOKERY_RUNTIME_ADAPTER_H */
