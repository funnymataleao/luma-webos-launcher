/*
 * The pipeline lifecycle, with the video plane left as a hole for each sample to fill.
 *
 * Everything about driving libplayerAPIs is the same on every webOS generation: build a
 * BUFFERSTREAM Load payload, call Load(), wait for LOADCOMPLETED, call Play() from inside
 * that callback, then Feed() access units until the file runs out. What differs is only
 * how the decoded picture gets onto the screen:
 *
 *   webOS 5+  an SDL exported window, whose id travels inside the Load payload
 *   webOS 2-4 libAcbAPI, told about the pipeline's media id out of band
 *   webOS 1   two luna calls to videosinkmanager and tv.display
 *
 * So that is the seam: smp_video_plane. Each sample supplies one, and its main.c stays
 * short enough to read in one sitting.
 */
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "smp_api.h"
#include "smp_payload.h"

typedef struct smp_video_plane {
    void *self;

    /* Before Load(), and before prepare_load: the pipeline's connection id is now known.
     * ACB needs it here rather than at LOADCOMPLETED - it has to be attached to the
     * pipeline before the pipeline is told to load. */
    void (*set_media_id)(void *self, const char *media_id);

    /* Before Load(): add whatever the plane needs to the payload. Returning false aborts
     * the load. webOS 5 fills in params->window_id here. */
    bool (*prepare_load)(void *self, smp_load_params *params);

    /* Load() returned true. The pipeline exists but has not reported readiness yet. */
    void (*post_load)(void *self, const smp_load_params *params);

    /* LOADCOMPLETED. `media_id` is the pipeline's connection id, which ACB and the webOS 1
     * luna calls both need. Runs on the pipeline's thread. */
    void (*load_completed)(void *self, const char *media_id);

    /* The first buffer was accepted, so the pipeline is really running. */
    void (*start_playing)(void *self);

    /* STR_VIDEO_INFO, verbatim JSON - carries the decoder's real picture geometry, which
     * can differ from what the sample guessed. Runs on the pipeline's thread. */
    void (*video_info)(void *self, const char *info_json);

    /* After Unload(). */
    void (*post_unload)(void *self);
} smp_video_plane;

typedef struct smp_player smp_player;

/* `plane` is copied; it may live on the caller's stack. */
smp_player *smp_player_create(const smp_video_plane *plane);
void smp_player_destroy(smp_player *player);

bool smp_player_load(smp_player *player, const smp_load_params *params);

/* Feeds one access unit. `data` must stay valid for the duration of the call.
 * SMP_FEED_BUFFER_FULL means "not now" - wait a moment and offer the same buffer again. */
smp_feed_result smp_player_feed(smp_player *player, const void *data, size_t size,
                                int64_t pts_ns, int es_data);

/* Tells the pipeline that no more buffers are coming. Call this once the last access unit
 * has been fed and *before* waiting for ENDOFSTREAM - the pipeline has no other way to
 * know the stream finished, so without it the wait always times out and the tail of the
 * clip is cut off. */
void smp_player_push_eos(smp_player *player);

/* pushEOS() (if not already done) then Unload(). Safe to call more than once. */
void smp_player_unload(smp_player *player);

/* Set once the pipeline reported ENDOFSTREAM. */
bool smp_player_ended(smp_player *player);

/* Set if the pipeline reported an error; the sample should stop feeding and tear down. */
bool smp_player_errored(smp_player *player);
