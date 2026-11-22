#ifndef ACGC_NOOKERY_RUNTIME_ADAPTER_H
#define ACGC_NOOKERY_RUNTIME_ADAPTER_H

#ifdef __cplusplus
extern "C" {
#endif

/* Diagnostic-only host callback path; it never replaces boot_main. */
int acgc_nookery_runtime_smoke(void);

#ifdef __cplusplus
}
#endif

#endif /* ACGC_NOOKERY_RUNTIME_ADAPTER_H */
