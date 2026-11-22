#ifndef ACGC_NOOKERY_RUNTIME_ADAPTER_H
#define ACGC_NOOKERY_RUNTIME_ADAPTER_H

#ifdef __cplusplus
extern "C" {
#endif

/* Production PC entry for the graph loop. The Rust runtime owns the frame
 * lifecycle; the callbacks below are the existing graph/game/audio seams. */
int acgc_nookery_runtime_graph_proc(void* arg);

#ifdef __cplusplus
}
#endif

#endif /* ACGC_NOOKERY_RUNTIME_ADAPTER_H */
