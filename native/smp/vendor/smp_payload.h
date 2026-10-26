/*
 * The two JSON documents the pipeline actually consumes.
 *
 * These are written with printf rather than a JSON library on purpose: the exact bytes are
 * the interesting part of a sample, and every key here was either read out of a shipped
 * webOS binary or copied from a known-working homebrew player. Hiding them behind a DOM
 * builder would defeat the point.
 */
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct smp_load_params {
    /* From getenv("APPID") - the launcher sets it for native apps. The pipeline uses it for
     * resource accounting, and refuses to play if it does not match a real app. */
    const char *app_id;

    /* webOS 5+ only: the id of an SDL exported window, placed at option.windowId. Leave
     * NULL on generations that attach the video sink some other way (ACB, or the webOS 1
     * luna calls) - the key is then omitted entirely. */
    const char *window_id;

    /* esInfo.pauseAtDecodeTime: hold the first decoded picture until ptsToDecode is
     * reached. True on every generation that has been made to work.
     *
     * It was suspected of causing the webOS 4.4 failure, because the pipeline prints
     * "prerolling state (play command pending)" right after parsing it. Setting it false
     * there changed the symptom - a delay, then a single frame, then exit, instead of an
     * immediate freeze - but did not fix it, so the suspicion is unproven and the default
     * stays as it is. */
    bool pause_at_decode_time;

    /* Ask the pipeline to start showing pictures as soon as it can, rather than filling a
     * buffer first. Costs resilience - there is nothing in hand if the source stalls - so
     * it suits a local file or a live stream, not a network player riding out jitter.
     *
     * Only partly effective, and only on newer firmware. See smp_payload.c: of the three
     * things it changes, webOS 4's player-factory recognises just one. */
    bool low_latency;

    bool has_video;
    const char *video_codec; /* "H264", "H265", "AV1" */
    int video_width;
    int video_height;
    int video_fps_num;
    int video_fps_den;

    bool has_audio;
    const char *audio_codec; /* "AAC", "AC3", "OPUS", "PCM" */
    int audio_channels;
    int audio_sample_rate;   /* Hz - the payload carries kHz, converted here */
    int aac_object_type;     /* 2 == AAC-LC */
} smp_load_params;

/* Writes the Load() payload. Returns false if it would not fit, in which case `out` must
 * not be sent - a truncated payload is not a valid JSON document. */
bool smp_payload_load(char *out, size_t out_len, const smp_load_params *params);

/* Writes the Feed() payload. `buffer` is passed to the pipeline as an address, which it
 * dereferences in-process, so the memory must stay valid across the Feed() call. */
bool smp_payload_feed(char *out, size_t out_len, const void *buffer, size_t size,
                      int64_t pts_ns, int es_data);
