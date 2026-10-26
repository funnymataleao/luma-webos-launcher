#include "smp_player.h"

#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "pacer.h"

/* The Load payload is a few hundred bytes; 4 KiB leaves room for long app ids. */
#define SMP_LOAD_PAYLOAD_MAX 4096
/* The Feed payload is a fixed five-field object. */
#define SMP_FEED_PAYLOAD_MAX 192

struct smp_player {
    smp_api *api;
    smp_video_plane plane;

    /* The Load callback runs on the pipeline's own thread, so everything it touches is
     * behind this lock. */
    pthread_mutex_t lock;
    /* Load() accepted the payload. This, not load_completed, is what gates feeding -
     * see smp_player_feed. */
    bool loaded;
    bool load_completed;
    bool ended;
    bool errored;

    bool eos_pushed;
    /* Play() has been issued. It can be triggered from either the pipeline's thread or the
     * feeding one, whichever gets there first - see ensure_playing(). */
    bool play_issued;
    bool playing;
    int64_t start_ns;
    int64_t frames_rendered;
};

static void plane_noop_post_load(void *self, const smp_load_params *params) {
    (void) self;
    (void) params;
}

static void plane_noop_str(void *self, const char *value) {
    (void) self;
    (void) value;
}

static void plane_noop(void *self) { (void) self; }

static bool plane_noop_prepare(void *self, smp_load_params *params) {
    (void) self;
    (void) params;
    return true;
}

static void load_callback(int type, int64_t num_value, const char *str_value, void *user);

/*
 * Starts playback exactly once.
 *
 * The obvious place to call Play() is on LOADCOMPLETED, and that is what every reference
 * implementation does - but the event is not dependable. On webOS 2.2 it does not arrive
 * until feeding stops, so a player that waits for it never starts: with
 * esInfo.pauseAtDecodeTime set the pipeline shows the first picture and then holds it,
 * which looks exactly like a decode failure.
 *
 * So whichever happens first wins - the event, or the first buffer the pipeline accepts.
 * By then Load() has returned true and data is flowing, which is enough.
 *
 * But "first" is not always "accepted": webOS 4.4 refuses an early Play() and only takes
 * one after LOADCOMPLETED, so the call is only recorded as done when it actually returns
 * true. Marking it done regardless leaves the pipeline sitting in its loading state
 * forever - visible in the platform log as a stream of
 * "checkAppSrcBuffer: not Play State" while every buffer is still being accepted.
 */
static void ensure_playing(smp_player *player) {
    pthread_mutex_lock(&player->lock);
    bool issue = !player->play_issued;
    pthread_mutex_unlock(&player->lock);
    if (!issue) {
        return;
    }

    bool ok = smp_api_play(player->api);
    fprintf(stderr, "[smp @%.0fms] Play() -> %s\n", pacer_uptime_ms(),
            ok ? "accepted" : "refused, will retry");
    if (!ok) {
        return;
    }
    pthread_mutex_lock(&player->lock);
    player->play_issued = true;
    pthread_mutex_unlock(&player->lock);
}

smp_player *smp_player_create(const smp_video_plane *plane) {
    smp_player *player = calloc(1, sizeof(smp_player));
    if (player == NULL) {
        return NULL;
    }
    player->plane = *plane;
    /* Filling the holes once here keeps every call site free of null checks. */
    if (player->plane.set_media_id == NULL) {
        player->plane.set_media_id = plane_noop_str;
    }
    if (player->plane.prepare_load == NULL) {
        player->plane.prepare_load = plane_noop_prepare;
    }
    if (player->plane.post_load == NULL) {
        player->plane.post_load = plane_noop_post_load;
    }
    if (player->plane.load_completed == NULL) {
        player->plane.load_completed = plane_noop_str;
    }
    if (player->plane.start_playing == NULL) {
        player->plane.start_playing = plane_noop;
    }
    if (player->plane.video_info == NULL) {
        player->plane.video_info = plane_noop_str;
    }
    if (player->plane.post_unload == NULL) {
        player->plane.post_unload = plane_noop;
    }

    pthread_mutex_init(&player->lock, NULL);

    /* The uid argument names the pipeline instance; NULL lets libplayerAPIs pick one. */
    player->api = smp_api_create(NULL);
    if (player->api == NULL) {
        fprintf(stderr, "[smp] cannot create StarfishMediaAPIs\n");
        pthread_mutex_destroy(&player->lock);
        free(player);
        return NULL;
    }
    return player;
}

void smp_player_destroy(smp_player *player) {
    if (player == NULL) {
        return;
    }
    smp_api_destroy(player->api);
    pthread_mutex_destroy(&player->lock);
    free(player);
}

bool smp_player_load(smp_player *player, const smp_load_params *params) {
    /* The pipeline refuses to load, play or accept buffers unless the app is in the
     * foreground, and it will not work that out on its own. */
    smp_api_notify_foreground(player->api);

    /* Available as soon as the pipeline object exists - it does not wait for Load(). */
    player->plane.set_media_id(player->plane.self, smp_api_media_id(player->api));

    smp_load_params local = *params;
    if (!player->plane.prepare_load(player->plane.self, &local)) {
        fprintf(stderr, "[smp] video plane refused to prepare\n");
        return false;
    }

    char payload[SMP_LOAD_PAYLOAD_MAX];
    if (!smp_payload_load(payload, sizeof(payload), &local)) {
        fprintf(stderr, "[smp] Load payload does not fit in %d bytes\n", SMP_LOAD_PAYLOAD_MAX);
        return false;
    }
    fprintf(stderr, "[smp @%.0fms] Load(%s)\n", pacer_uptime_ms(), payload);

    if (!smp_api_load(player->api, payload, load_callback, player)) {
        fprintf(stderr, "[smp] Load failed\n");
        return false;
    }
    pthread_mutex_lock(&player->lock);
    player->loaded = true;
    pthread_mutex_unlock(&player->lock);

    player->plane.post_load(player->plane.self, &local);
    return true;
}

smp_feed_result smp_player_feed(smp_player *player, const void *data, size_t size,
                                int64_t pts_ns, int es_data) {
    pthread_mutex_lock(&player->lock);
    bool errored = player->errored;
    bool ready = player->loaded;
    pthread_mutex_unlock(&player->lock);

    if (errored) {
        return SMP_FEED_ERROR;
    }
    if (!ready) {
        return SMP_FEED_ERROR;
    }
    /* Note what is *not* checked here: LOADCOMPLETED. Feeding starts as soon as Load()
     * returns, because the pipeline will not finish loading until it has data - it
     * prerolls on the first buffers and only then reports LOADCOMPLETED. Waiting for the
     * event before feeding deadlocks: the pipeline sits in LoadingState forever, and the
     * app sits waiting for an event that needs a buffer to arrive first. */

    char payload[SMP_FEED_PAYLOAD_MAX];
    if (!smp_payload_feed(payload, sizeof(payload), data, size, pts_ns, es_data)) {
        return SMP_FEED_ERROR;
    }

    smp_feed_result result = smp_api_feed(player->api, payload);
    if (result == SMP_FEED_ERROR) {
        fprintf(stderr, "[smp] Feed(esData=%d, %zu bytes) -> %s\n", es_data, size,
                smp_api_last_feed_reply(player->api));
        return result;
    }
    if (result == SMP_FEED_OK) {
        pthread_mutex_lock(&player->lock);
        bool first = !player->playing;
        if (first) {
            player->playing = true;
            player->start_ns = pacer_now_ns();
        }
        pthread_mutex_unlock(&player->lock);
        if (first) {
            ensure_playing(player);
            player->plane.start_playing(player->plane.self);
        }
    }
    return result;
}

void smp_player_push_eos(smp_player *player) {
    pthread_mutex_lock(&player->lock);
    bool send = player->loaded && !player->eos_pushed;
    player->eos_pushed = true;
    pthread_mutex_unlock(&player->lock);

    if (send) {
        smp_api_push_eos(player->api);
    }
}

void smp_player_unload(smp_player *player) {
    pthread_mutex_lock(&player->lock);
    bool loaded = player->loaded;
    bool eos_pushed = player->eos_pushed;
    player->loaded = false;
    player->play_issued = true;
    player->eos_pushed = true;
    player->load_completed = false;
    pthread_mutex_unlock(&player->lock);

    if (!loaded) {
        return;
    }
    /* Drain rather than cut off mid-buffer. Normally the sample has already done this
     * before waiting for ENDOFSTREAM; this covers the error paths. */
    if (!eos_pushed) {
        smp_api_push_eos(player->api);
    }
    smp_api_unload(player->api);
    player->plane.post_unload(player->plane.self);
}

bool smp_player_ended(smp_player *player) {
    pthread_mutex_lock(&player->lock);
    bool ended = player->ended;
    pthread_mutex_unlock(&player->lock);
    return ended;
}

bool smp_player_errored(smp_player *player) {
    pthread_mutex_lock(&player->lock);
    bool errored = player->errored;
    pthread_mutex_unlock(&player->lock);
    return errored;
}

static void load_callback(int type, int64_t num_value, const char *str_value, void *user) {
    smp_player *player = user;

    switch (type) {
        case SMP_EVENT_STR_STATE_UPDATE_LOADCOMPLETED: {
            /* Play() belongs here, not after Load() returns: the pipeline is only ready to
             * be started once it says so. */
            pthread_mutex_lock(&player->lock);
            player->load_completed = true;
            pthread_mutex_unlock(&player->lock);

            const char *media_id = smp_api_media_id(player->api);
            fprintf(stderr, "[smp @%.0fms] load completed, mediaId=%s\n", pacer_uptime_ms(),
                    media_id ? media_id : "(none)");
            player->plane.load_completed(player->plane.self, media_id);
            /*
             * Unconditionally, even if Play() has already been issued and accepted.
             *
             * A Play() sent before LOADCOMPLETED can be *queued* rather than executed - the
             * pipeline logs "prerolling state (play command pending)" and returns true - and
             * the queued command is not always run once preroll finishes. Returning true
             * therefore says nothing about whether playback started, so the call is repeated
             * at the one moment the pipeline is definitely ready. Playing an already-playing
             * pipeline is harmless.
             */
            player->play_issued = false;
            ensure_playing(player);
            break;
        }
        case SMP_EVENT_FRAMEREADY: {
            /* num_value is the presented frame's PTS in nanoseconds. Against our own feed
             * clock that gives end-to-end latency - only reported occasionally, because
             * this fires once per frame. */
            pthread_mutex_lock(&player->lock);
            int64_t start = player->start_ns;
            int64_t n = player->frames_rendered++;
            pthread_mutex_unlock(&player->lock);
            if (n == 0) {
                /* The first picture to reach the screen. Its own timestamp says how much
                 * of the stream the pipeline swallowed before showing anything - that is
                 * the startup wait a viewer actually sees, and it is not the same as how
                 * late the frame is against the feed clock. */
                fprintf(stderr, "[smp @%.0fms] first picture on screen, its pts %.2fs "
                                "(so %.2fs of media went by unseen)\n",
                        pacer_uptime_ms(), (double) num_value / 1e9,
                        (double) num_value / 1e9);
            }
            if (start != 0 && n % 120 == 0) {
                int64_t elapsed = pacer_now_ns() - start;
                fprintf(stderr, "[smp @%.0fms] frame %lld rendered, %.1f ms behind feed "
                                "clock\n",
                        pacer_uptime_ms(), (long long) n,
                        (double) (elapsed - num_value) / 1e6);
            }
            break;
        }
        case SMP_EVENT_STR_VIDEO_INFO:
            fprintf(stderr, "[smp] videoInfo %s\n", str_value ? str_value : "");
            player->plane.video_info(player->plane.self, str_value);
            break;
        case SMP_EVENT_STR_AUDIO_INFO:
            fprintf(stderr, "[smp] audioInfo %s\n", str_value ? str_value : "");
            break;
        case SMP_EVENT_STR_STATE_UPDATE_ENDOFSTREAM:
            fprintf(stderr, "[smp] end of stream\n");
            pthread_mutex_lock(&player->lock);
            player->ended = true;
            pthread_mutex_unlock(&player->lock);
            break;
        case SMP_EVENT_STR_STATE_UPDATE_UNLOADCOMPLETED:
            fprintf(stderr, "[smp] unload completed\n");
            break;
        case SMP_EVENT_INT_ERROR:
        case SMP_EVENT_STR_ERROR:
            fprintf(stderr, "[smp] pipeline error %lld %s\n", (long long) num_value,
                    str_value ? str_value : "");
            /* 601 is worth calling out by name. It means the decoder could not be
             * allocated - almost always because something else still holds it: another
             * media app, or an earlier run of this one that died without unloading. The
             * pipeline logs "resourceStructure is NULL" alongside it. Nothing the app can
             * do about it except stop; the resources come back when the holder exits, or
             * on a reboot. */
            if (num_value == 601) {
                fprintf(stderr, "[smp] the video decoder is held by something else - close "
                                "other media apps, or reboot the TV if a previous run "
                                "crashed without unloading\n");
            }
            pthread_mutex_lock(&player->lock);
            player->errored = true;
            pthread_mutex_unlock(&player->lock);
            break;
        case SMP_EVENT_STR_BUFFERFULL:
            /* Informational. Back-pressure is handled from Feed()'s reply, which is
             * synchronous and therefore actionable; this event is not. */
            break;
        case SMP_EVENT_DROPPED_FRAME:
            fprintf(stderr, "[smp] dropped frame\n");
            break;
        default:
            break;
    }
}
