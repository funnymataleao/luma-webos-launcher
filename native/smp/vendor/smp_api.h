/*
 * A C façade over StarfishMediaAPIs, the C++ class exported by libplayerAPIs.
 *
 * Everything else in these samples is C. This header, and the single translation unit
 * behind it, exist because the pipeline's entry point is a C++ class whose ABI is not
 * stable across webOS generations. Confining that to one file means the generation
 * differences are all in one place - see smp_api.cpp and the CMakeLists next to it.
 *
 * The differences, verified by diffing exported symbols of libplayerAPIs.so across
 * retail firmware images:
 *
 *   webOS 1      old std::string ABI, no-argument constructor, Load() without a user
 *                pointer, no notifyForeground()
 *   webOS 2      old std::string ABI, Load() without a user pointer
 *   webOS 3      old std::string ABI
 *   webOS 4+     __cxx11 std::string ABI
 *
 * Selected with SMP_CTOR_NO_UID / SMP_LOAD_HAS_USERDATA and, for the string ABI,
 * -D_GLIBCXX_USE_CXX11_ABI=0 on the whole translation unit.
 */
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Event ids delivered to the Load callback. Same values as PF_EVENT_T in the SDK header;
 * only the ones the samples act on are named here. */
enum {
    SMP_EVENT_FRAMEREADY = 0x00,
    SMP_EVENT_STR_VIDEO_INFO = 0x04,
    SMP_EVENT_STR_AUDIO_INFO = 0x07,
    SMP_EVENT_STR_SOURCE_INFO = 0x0b,
    SMP_EVENT_INT_ERROR = 0x12,
    SMP_EVENT_STR_ERROR = 0x13,
    SMP_EVENT_STR_STATE_UPDATE_LOADCOMPLETED = 0x16,
    SMP_EVENT_STR_STATE_UPDATE_UNLOADCOMPLETED = 0x17,
    SMP_EVENT_STR_STATE_UPDATE_PLAYING = 0x1a,
    SMP_EVENT_STR_STATE_UPDATE_PAUSED = 0x1b,
    SMP_EVENT_STR_STATE_UPDATE_ENDOFSTREAM = 0x1c,
    SMP_EVENT_INT_BUFFERLOW = 0x2c,
    SMP_EVENT_STR_BUFFERFULL = 0x2d,
    SMP_EVENT_STR_BUFFERLOW = 0x2e,
    SMP_EVENT_DROPPED_FRAME = 0x30,
};

/* Which elementary stream a Feed() call carries. */
enum {
    SMP_ES_VIDEO = 1,
    SMP_ES_AUDIO = 2,
};

/* Feed() answers with a status *string*, not a boolean. BufferFull is ordinary
 * back-pressure and means "retry this same buffer shortly", not "give up". */
typedef enum smp_feed_result {
    SMP_FEED_OK,
    SMP_FEED_BUFFER_FULL,
    SMP_FEED_PENDING,
    SMP_FEED_ERROR,
} smp_feed_result;

typedef struct smp_api smp_api;

/* Invoked on the pipeline's own thread, not the caller's. */
typedef void (smp_event_fn)(int type, int64_t num_value, const char *str_value, void *user);

smp_api *smp_api_create(const char *uid);
void smp_api_destroy(smp_api *api);

bool smp_api_load(smp_api *api, const char *payload, smp_event_fn *callback, void *user);
bool smp_api_play(smp_api *api);
bool smp_api_pause(smp_api *api);
bool smp_api_push_eos(smp_api *api);
bool smp_api_unload(smp_api *api);

/* Absent on webOS 1; resolved weakly, and reports false there rather than failing to link. */
bool smp_api_notify_foreground(smp_api *api);

/* The pipeline's connection id. ACB and the webOS 1 luna calls both need it to attach the
 * video sink to this pipeline, so it must be read after create() and before Load(). */
const char *smp_api_media_id(smp_api *api);

smp_feed_result smp_api_feed(smp_api *api, const char *payload);

/* Human-readable form of the last Feed() reply, for logging. */
const char *smp_api_last_feed_reply(const smp_api *api);

#ifdef __cplusplus
}
#endif
