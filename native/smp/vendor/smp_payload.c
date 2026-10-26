#include "smp_payload.h"

#include <stdio.h>

#include "strbuf.h"

/*
 * Buffer levels, in bytes, handed to the pipeline's appsrc at Load time.
 *
 * These are the defaults a shipped webOS media app uses for file-like playback. A
 * low-latency streaming client would set them far lower - down around 1 MiB of video - and
 * trade smoothness for delay. For playing a file off disk, roomier is better: it is what
 * keeps the samples from bouncing off BufferFull on every other access unit.
 */
#define SMP_SRC_BUFFER_VIDEO_MAX (8 * 1024 * 1024)
#define SMP_SRC_BUFFER_AUDIO_MAX (1 * 1024 * 1024)
#define SMP_SRC_BUFFER_MIN 1024

/* The low-latency pair. Small enough that the pipeline cannot sit on a backlog, which is
 * the whole point: it starts rendering as soon as there is something to render. These are
 * the levels a low-latency streaming client uses, and unlike the newer options below they
 * are recognised on every generation. */
#define SMP_SRC_BUFFER_VIDEO_MAX_LOW (1 * 1024 * 1024)
#define SMP_SRC_BUFFER_AUDIO_MAX_LOW (32 * 1024)

bool smp_payload_load(char *out, size_t out_len, const smp_load_params *params) {
    strbuf sb;
    strbuf_init(&sb, out, out_len);

    /* Every generation wraps the real argument in a one-element "args" array. */
    strbuf_addf(&sb, "{\"args\":[{");
    strbuf_addf(&sb, "\"mediaTransportType\":\"BUFFERSTREAM\",");
    strbuf_addf(&sb, "\"option\":{");

    strbuf_addf(&sb, "\"appId\":");
    strbuf_add_json_string(&sb, params->app_id);

    if (params->window_id != NULL) {
        strbuf_addf(&sb, ",\"windowId\":");
        strbuf_add_json_string(&sb, params->window_id);
    }

    /* With queryPosition true the pipeline reports playback position instead of sending
     * FRAMEREADY. The samples want FRAMEREADY, to show end-to-end render latency. */
    strbuf_addf(&sb, ",\"queryPosition\":false");

    strbuf_addf(&sb, ",\"externalStreamingInfo\":{");
    strbuf_addf(&sb, "\"contents\":{");

    strbuf_addf(&sb, "\"codec\":{");
    if (params->has_video) {
        strbuf_addf(&sb, "\"video\":");
        strbuf_add_json_string(&sb, params->video_codec);
    }
    if (params->has_audio) {
        strbuf_addf(&sb, "%s\"audio\":", params->has_video ? "," : "");
        strbuf_add_json_string(&sb, params->audio_codec);
    }
    strbuf_addf(&sb, "}");

    strbuf_addf(&sb, ",\"esInfo\":{");
    /* pauseAtDecodeTime + ptsToDecode hold the first picture until a chosen timestamp;
     * 0 means "start as soon as there is something to show".
     * seperatedPTS - the misspelling is in the API - says audio and video timestamps are
     * independent values on a shared time base, which is exactly our two-file case.
     * (On webOS 1, ptsToDecode is a *string* formatted "%lld000000", not a number.) */
    strbuf_addf(&sb, "\"pauseAtDecodeTime\":%s,\"ptsToDecode\":0,\"seperatedPTS\":true",
                params->pause_at_decode_time ? "true" : "false");
    if (params->has_video) {
        strbuf_addf(&sb, ",\"videoFpsValue\":%d,\"videoFpsScale\":%d", params->video_fps_num,
                    params->video_fps_den);
    }
    strbuf_addf(&sb, "}");

    /* RAW: we hand over elementary streams, not a container. */
    strbuf_addf(&sb, ",\"format\":\"RAW\"");
    /* provider is matched against a known list; "Chrome" is the value homebrew players use
     * and behaves as a generic buffer-stream source. */
    strbuf_addf(&sb, ",\"provider\":\"Chrome\"");

    if (params->has_audio) {
        /* frequency is in kHz, and profile is the MPEG-4 audio object type (2 == AAC-LC).
         * format "adts" tells the pipeline each frame carries its own header, which is why
         * the sample can feed the file without any out-of-band configuration. */
        strbuf_addf(&sb, ",\"aacInfo\":{\"channels\":%d,\"format\":\"adts\",\"frequency\":%d,"
                         "\"profile\":%d}",
                    params->audio_channels, params->audio_sample_rate / 1000,
                    params->aac_object_type);
    }
    strbuf_addf(&sb, "}");/* contents */

    strbuf_addf(&sb, ",\"restartStreaming\":false");
    strbuf_addf(&sb, ",\"streamQualityInfo\":true");
    strbuf_addf(&sb, ",\"streamQualityInfoNonFlushable\":true");
    strbuf_addf(&sb, ",\"totalStreamSize\":256");

    strbuf_addf(&sb, ",\"bufferingCtrInfo\":{");
    strbuf_addf(&sb, "\"preBufferByte\":0,\"bufferMinLevel\":0,\"bufferMaxLevel\":0,");
    strbuf_addf(&sb, "\"qBufferLevelAudio\":0,\"qBufferLevelVideo\":0,");
    strbuf_addf(&sb, "\"srcBufferLevelAudio\":{\"minimum\":%d,\"maximum\":%d},",
                SMP_SRC_BUFFER_MIN, SMP_SRC_BUFFER_AUDIO_MAX);
    strbuf_addf(&sb, "\"srcBufferLevelVideo\":{\"minimum\":%d,\"maximum\":%d}",
                SMP_SRC_BUFFER_MIN, SMP_SRC_BUFFER_VIDEO_MAX);
    strbuf_addf(&sb, "}");/* bufferingCtrInfo */

    strbuf_addf(&sb, "}");/* externalStreamingInfo */

    if (params->low_latency) {
        /*
         * Of the three low-latency levers, only the buffer levels above are universal.
         * These two are newer, and it is worth knowing which firmware ignores them rather
         * than assuming they help: libpf on the TV contains the complete list of option
         * paths it parses, so
         *
         *     strings /usr/lib/libpf-1.0.so.1.0.0 | grep '^option\.'
         *
         * answers the question directly. On webOS 4.4 that list has no "lowDelayMode" at
         * all, and no "WEBRTC" string for contentsType to match - so both are inert there,
         * which is exactly what measuring showed: first picture at 950ms either way.
         */
        strbuf_addf(&sb, ",\"transmission\":{\"contentsType\":\"WEBRTC\"}");
        strbuf_addf(&sb, ",\"lowDelayMode\":true");
    }

    if (params->has_video) {
        /* Tells the decoder the ceiling it must be able to handle, so it can reserve the
         * right resources up front. */
        strbuf_addf(&sb, ",\"adaptiveStreaming\":{\"audioOnly\":false,\"maxWidth\":%d,"
                         "\"maxHeight\":%d,\"maxFrameRate\":%d}",
                    params->video_width, params->video_height,
                    params->video_fps_num / params->video_fps_den);
    }

    strbuf_addf(&sb, "}");  /* option */
    strbuf_addf(&sb, "}]}");/* arg, args */

    return strbuf_str(&sb) != NULL;
}

bool smp_payload_feed(char *out, size_t out_len, const void *buffer, size_t size,
                      int64_t pts_ns, int es_data) {
    /* bufferAddr is the host pointer rendered as a JSON *string*. The pipeline lives in
     * this process, reads the bytes during the call, and is done with them when it
     * returns - so nothing needs to outlive Feed(). */
    int written = snprintf(out, out_len,
                           "{\"bufferAddr\":\"%p\",\"bufferSize\":%zu,\"pts\":%lld,"
                           "\"esData\":%d}",
                           buffer, size, (long long) pts_ns, es_data);
    return written > 0 && (size_t) written < out_len;
}
