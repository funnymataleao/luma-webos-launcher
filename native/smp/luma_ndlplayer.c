/* Experimental LG NDL DirectMedia 2 hardware video wallpaper.
 * ABI-compatible with src/hw_wallpaper.rs; video-only, no RGB readback.
 * Luma photo fallback is kept until the TV reports a decoded frame.
 */
#define NDL_DIRECTMEDIA_API_VERSION 2
#include <NDL_directmedia.h>
#include <stdlib.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <sys/stat.h>
#include <limits.h>
#include <time.h>

typedef struct {
    int initialized;
    int loaded;
    int64_t frames;
    unsigned long fed;
    int playing;
    int errored;
    unsigned long unknown_events;
    unsigned long raw_negative;
    unsigned long prev_negative;
    int last_unknown_event;
    unsigned long prev_frames;
    unsigned long prev_unknown;
    unsigned long queue_count;
    long long queue_total;
    int queue_min;
    int queue_max;
} LumaNDL;
static LumaNDL *active=NULL;
static void record(const char *label,int64_t value) {
    FILE *f=fopen("/media/developer/com.denisefremov.luma/luma-ndl-events.log","a");
    if(f){fprintf(f,"%s %lld\n",label,(long long)value);fclose(f);}
}
static void resource_released(const char *reason) {
    record("RESOURCE_RELEASED",0);
    if(active)__atomic_store_n(&active->errored,1,__ATOMIC_RELEASE);
}
static void callback(int type,long long value,const char *str) {
    if(type==0) {
        if(active){
            int64_t n=__atomic_add_fetch(&active->frames,1,__ATOMIC_SEQ_CST);
            if(n==1 || n%120==0)record("FRAMEREADY",value);
        }
    } else if(type==0x16)record("LOADCOMPLETED",0);
    else if(type==0x1a){record("PLAYING",0);if(active)__atomic_store_n(&active->playing,1,__ATOMIC_RELEASE);}
    else if(type==0x12 || type==0x13){
        record("ERROR",value);
        if(active)__atomic_store_n(&active->errored,1,__ATOMIC_RELEASE);
    } else if(type==0x1c)record("EOS",0);
    else {
        record("UNKNOWN_CALLBACK",type);
        if(active) {
            __atomic_add_fetch(&active->unknown_events,1,__ATOMIC_RELAXED);
            __atomic_store_n(&active->last_unknown_event,type,__ATOMIC_RELAXED);
        }
    }
}
void *luma_hw_open(const char *app_id,int width,int height,const char *codec) {
    if(!app_id||width<=0||height<=0||active)return NULL;
    LumaNDL *p=calloc(1,sizeof(*p));if(!p)return NULL;
    active=p;
    p->queue_min=INT_MAX;
    record("OPEN",0);
    if(NDL_DirectMediaInit(app_id,resource_released)!=0){
        record("INIT_FAILED",-1);goto fail;
    }
    p->initialized=1;
    NDL_DirectMediaSetAppState(NDL_DIRECTMEDIA_APP_STATE_FOREGROUND);
    NDL_DIRECTMEDIA_DATA_INFO_T info={0};
    info.video.width=width;
    info.video.height=height;
    info.video.type=(codec && !strcmp(codec,"H265"))?NDL_VIDEO_TYPE_H265:NDL_VIDEO_TYPE_H264;
    // NDL v2 requires an audio plane to pace the video correctly. A single
    // silent 5ms stereo Opus packet primes it; no audible sound is emitted.
    // Sample-rate here is in *kHz*, not Hz (webOS NDL ABI).
    info.audio.opus.type=NDL_AUDIO_TYPE_OPUS;
    info.audio.opus.channels=2;
    info.audio.opus.sampleRate=48.0;
    info.audio.opus.streamHeader=NULL;
    if(NDL_DirectMediaLoad(&info,callback)!=0){
        record("LOAD_FAILED",-1);goto fail;
    }
    p->loaded=1;
    const unsigned char silent_opus_5ms[3]={0xec,0xff,0xfe};
    int prime_ok=0;
    for(int i=0;i<8;i++){
        int64_t pts_ms=(int64_t)i*5;
        int result=NDL_DirectAudioPlay((void*)silent_opus_5ms,3,pts_ms);
        if(result==0)prime_ok++;
    }
    record("AUDIO_PRIME_ACCEPTED",prime_ok);
    // NDL v2 self-sizes the hardware video punch-through plane.
    // DirectVideoSetArea is v1-only; on v2 it causes stutter above 1080p.
    record("LOAD_OK",info.video.type);
    return p;
fail:
    if(p->loaded)NDL_DirectMediaUnload();
    if(p->initialized)NDL_DirectMediaQuit();
    free(p);active=NULL;return NULL;
}
int luma_hw_feed(void *opaque,const void *buffer,size_t size,int64_t pts_ns) {
    LumaNDL *p=opaque;
    if(!p || !p->loaded || __atomic_load_n(&p->errored,__ATOMIC_ACQUIRE) || !buffer || !size)return -1;
    int result=NDL_DirectVideoPlay((void*)buffer,(unsigned int)size,pts_ns/1000000);
    if(result==0){
        __atomic_add_fetch(&p->fed,1,__ATOMIC_RELEASE);
        // Sample rendering backlog at a low rate: diagnostic only.
        if(p->fed%5==0){
            int queued=-1;
            if(NDL_DirectVideoGetRenderBufferLength(&queued)==0){
                p->queue_count++;
                p->queue_total+=queued;
                if(queued<p->queue_min)p->queue_min=queued;
                if(queued>p->queue_max)p->queue_max=queued;
            }
        }
        if(p->fed==1 || p->fed%30==0){
            int queued=-1;
            int rc=NDL_DirectVideoGetRenderBufferLength(&queued);
            record("RENDER_QUEUE",rc==0?queued:-1000+rc);
            if(p->fed==1 || p->fed%150==0)record("FEED_OK",p->fed);
        }
        return 0;
    }
    if(result<0)__atomic_add_fetch(&p->raw_negative,1,__ATOMIC_RELAXED);
    if(p->fed==0 || p->fed%120==0)record("FEED_BACKPRESSURE",result);
    return 1;
}
// One compact line per second, capped to 1 MiB with a single backup.
// Called only from the reader thread; callbacks use atomic counters.
void luma_hw_metrics(void *opaque,int index,const char *source,const char *codec,
                     unsigned au,unsigned retries,unsigned errors,double avg_late_ms,
                     double max_late_ms,size_t max_au) {
    LumaNDL *p=opaque;
    if(!p)return;
    const char *path="/media/developer/com.denisefremov.luma/luma-video-metrics.log";
    struct stat st;
    if(stat(path,&st)==0 && st.st_size>1024*1024){
        rename(path,"/media/developer/com.denisefremov.luma/luma-video-metrics.prev.log");
    }
    FILE *f=fopen(path,"a");
    if(!f)return;
    unsigned long frames=__atomic_load_n(&p->frames,__ATOMIC_ACQUIRE);
    unsigned long unknown=__atomic_load_n(&p->unknown_events,__ATOMIC_ACQUIRE);
    unsigned long negative=__atomic_load_n(&p->raw_negative,__ATOMIC_ACQUIRE);
    unsigned long count=p->queue_count;
    struct timespec now={0};
    clock_gettime(CLOCK_MONOTONIC,&now);
    long long elapsed_ms=(long long)now.tv_sec*1000+now.tv_nsec/1000000;
    fprintf(f,"METRIC t_ms=%lld idx=%d src=%s codec=%s au=%u retry=%u error=%u "
            "late_avg_ms=%.2f late_max_ms=%.2f max_au=%lu "
            "q_min=%d q_avg=%.2f q_max=%d q_samples=%lu "
            "frameready=%lu unknown=%lu unknown_type=%d ndl_negative=%lu\n",
            elapsed_ms,index,source,codec,au,retries,errors,avg_late_ms,max_late_ms,
            (unsigned long)max_au,count?p->queue_min:-1,
            count?(double)p->queue_total/count:-1.0,count?p->queue_max:-1,
            count,frames-p->prev_frames,unknown-p->prev_unknown,
            __atomic_load_n(&p->last_unknown_event,__ATOMIC_RELAXED),
            negative-p->prev_negative);
    fclose(f);
    p->prev_frames=frames;
    p->prev_unknown=unknown;
    p->prev_negative=negative;
    p->queue_count=0;
    p->queue_total=0;
    p->queue_min=INT_MAX;
    p->queue_max=0;
}
int luma_hw_set_drop_threshold(void *opaque,int threshold) {
    LumaNDL *p=opaque;
    if(!p || !p->loaded)return -1;
    int rc=NDL_DirectVideoSetFrameDropThreshold(threshold);
    record("DROP_THRESHOLD",rc);
    return rc;
}
int64_t luma_hw_frames_rendered(void *opaque) {
    LumaNDL *p=opaque;
    if(!p)return 0;
    int64_t confirmed=__atomic_load_n(&p->frames,__ATOMIC_SEQ_CST);
    if(confirmed>0)return confirmed;
    // NDL DirectMedia v2 reports PLAYING but may omit FRAMEREADY entirely
    // (verified on webOS 6). After buffered frames have been consumed by the
    // hardware render queue, allow the video plane through the UI alpha.
    int queued=-1;
    if(!__atomic_load_n(&p->errored,__ATOMIC_ACQUIRE)
        && __atomic_load_n(&p->playing,__ATOMIC_ACQUIRE)
        && __atomic_load_n(&p->fed,__ATOMIC_ACQUIRE)>20
        && NDL_DirectVideoGetRenderBufferLength(&queued)==0 && queued<=4)
        return (int64_t)p->fed;
    return 0;
}
int luma_hw_ended(void *opaque) {
    LumaNDL *p=opaque;
    return !p || __atomic_load_n(&p->errored,__ATOMIC_ACQUIRE);
}
void luma_hw_close(void *opaque) {
    LumaNDL *p=opaque;
    if(!p)return;
    record("CLOSE",p->fed);
    if(p->loaded)NDL_DirectMediaUnload();
    if(p->initialized)NDL_DirectMediaQuit();
    active=NULL;
    free(p);
}
