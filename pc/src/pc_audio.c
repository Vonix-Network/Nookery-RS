/* SDL3 host adapter for the Rust implementation in nookery-audio.
 *
 * The AI/DSP state and production call surface live in Rust.  This file is
 * deliberately limited to SDL/device operations so the Rust ABI remains
 * callable by the existing C and C++ game code.
 *
 * Oracle: the former implementation and its observable decisions are
 * pc/src/pc_audio.c symbols AIInit, AIInitDMA, AIStartDMA, AIStopDMA,
 * pc_audio_callback, pc_audio_get_buffer_fill, and pc_audio_shutdown.
 */

#include <SDL3/SDL.h>

#include "acgc/nookery_audio.h"

#include <stdio.h>

#define PC_AUDIO_SAMPLE_RATE 32000

static SDL_AudioStream* audio_stream;
static NookeryAudioRefillCallback refill_callback;

static void SDLCALL pc_audio_callback(void* userdata, SDL_AudioStream* stream,
                                      int additional_amount, int total_amount)
{
    (void)userdata;
    (void)stream;
    (void)total_amount;

    if (refill_callback != NULL) {
        refill_callback(additional_amount);
    }
}

bool nookery_audio_host_init(NookeryAudioRefillCallback callback)
{
    if (audio_stream != NULL) {
        return true;
    }

    if (!SDL_InitSubSystem(SDL_INIT_AUDIO)) {
        fprintf(stderr, "[audio] SDL audio initialization failed: %s\n", SDL_GetError());
        return false;
    }

    const SDL_AudioSpec spec = {
        .format = SDL_AUDIO_S16,
        .channels = 2,
        .freq = PC_AUDIO_SAMPLE_RATE,
    };
    audio_stream = SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec,
                                             pc_audio_callback, NULL);
    if (audio_stream == NULL) {
        fprintf(stderr, "[audio] Could not open output: %s\n", SDL_GetError());
        return false;
    }

    refill_callback = callback;
    return true;
}

void nookery_audio_host_set_gain(float gain)
{
    if (audio_stream != NULL) {
        (void)SDL_SetAudioStreamGain(audio_stream, gain);
    }
}

bool nookery_audio_host_put(const void* data, uint32_t size)
{
    if (audio_stream == NULL || data == NULL || size == 0) {
        return false;
    }
    if (!SDL_PutAudioStreamData(audio_stream, data, (int)size)) {
        fprintf(stderr, "[audio] Could not queue samples: %s\n", SDL_GetError());
        return false;
    }
    return true;
}

bool nookery_audio_host_resume(void)
{
    if (audio_stream == NULL) {
        return false;
    }
    if (!SDL_ResumeAudioStreamDevice(audio_stream)) {
        return false;
    }
    return true;
}

bool nookery_audio_host_pause(void)
{
    if (audio_stream == NULL) {
        return false;
    }
    if (!SDL_PauseAudioStreamDevice(audio_stream)) {
        return false;
    }
    return true;
}

int nookery_audio_host_available(void)
{
    if (audio_stream == NULL) {
        return 0;
    }
    return SDL_GetAudioStreamAvailable(audio_stream);
}

void nookery_audio_host_shutdown(void)
{
    if (audio_stream != NULL) {
        SDL_DestroyAudioStream(audio_stream);
        audio_stream = NULL;
    }
    refill_callback = NULL;
}
