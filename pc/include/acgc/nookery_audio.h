#ifndef ACGC_NOOKERY_AUDIO_H
#define ACGC_NOOKERY_AUDIO_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Narrow SDL host seam used by rust/nookery-audio. */
typedef void (*NookeryAudioRefillCallback)(int additional_amount);

bool nookery_audio_host_init(NookeryAudioRefillCallback callback);
void nookery_audio_host_set_gain(float gain);
bool nookery_audio_host_put(const void* data, uint32_t size);
bool nookery_audio_host_resume(void);
bool nookery_audio_host_pause(void);
int nookery_audio_host_available(void);
void nookery_audio_host_shutdown(void);

#ifdef __cplusplus
}
#endif

#endif
