// Hardware wallpaper for webOS 5+ / 6.
// The picture lives on a Starfish video plane through an SDL exported window.
// No RGB readback or video texture uploads.
#include <SDL.h>
#include <SDL_system.h>
#include "smp_player.h"
#include "json_scan.h"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct LumaHW {
    char window[128];
    smp_player *player;
    int screen_w, screen_h;
    int width, height;
    unsigned long packets;
    unsigned long retry_count;
    FILE *log;
} LumaHW;
static void report(LumaHW *ctx,const char *text) {
    if(ctx && ctx->log) {fprintf(ctx->log,"[LUMA HARDWARE] %s\n",text);fflush(ctx->log);}
}
static void geometry(LumaHW *ctx) {
    if(!ctx || !ctx->window[0]) return;
    SDL_Rect src={0,0,ctx->width,ctx->height};
    SDL_Rect dst={0,0,ctx->screen_w,ctx->screen_h};
    SDL_bool ok=SDL_webOSSetExportedWindow(ctx->window,&src,&dst);
    if(ctx->log){fprintf(ctx->log,"[LUMA HARDWARE] geometry %dx%d -> %dx%d: %d\n",
      src.w,src.h,dst.w,dst.h,(int)ok);fflush(ctx->log);}
}
static bool prepare_load(void *opaque,smp_load_params *params) {
    LumaHW *ctx=opaque;params->window_id=ctx->window;
    return ctx->window[0]!=0;
}
static void post_load(void *opaque,const smp_load_params *params) {
    LumaHW *ctx=opaque;ctx->width=params->video_width;ctx->height=params->video_height;
    geometry(ctx);
}
static void video_info(void *opaque,const char *json) {
    LumaHW *ctx=opaque;int width=0,height=0;
    if(json_scan_int(json,"width",&width)&&json_scan_int(json,"height",&height)&&width>0&&height>0){
        ctx->width=width;ctx->height=height;geometry(ctx);
    }
}
void *luma_hw_open(const char *app_id,int width,int height,const char *codec) {
    if(!app_id || width<=0 || height<=0) return NULL;
    LumaHW *ctx=calloc(1,sizeof(*ctx));if(!ctx)return NULL;
    ctx->width=width;ctx->height=height;
    ctx->log=fopen("/media/developer/com.denisefremov.luma/luma-hardware.log","a");
    ctx->screen_w=1920;ctx->screen_h=1080;
    SDL_webOSGetPanelResolution(&ctx->screen_w,&ctx->screen_h);
    const char *id=SDL_webOSCreateExportedWindow(SDL_WEBOS_EXPORTED_WINDOW_TYPE_VIDEO);
    if(!id){report(ctx,"SDL_webOSCreateExportedWindow failed");goto fail;}
    snprintf(ctx->window,sizeof(ctx->window),"%s",id);
    report(ctx,"Exported video plane window created");
    smp_video_plane plane={0};plane.self=ctx;plane.prepare_load=prepare_load;
    plane.post_load=post_load;plane.video_info=video_info;
    report(ctx,"SMP creating player");
    ctx->player=smp_player_create(&plane);
    report(ctx,"SMP player returned");
    if(!ctx->player){report(ctx,"smp_player_create failed");goto fail;}
    smp_load_params params={0};
    params.app_id=app_id;params.has_video=true;params.video_codec=codec && !strcmp(codec,"H265") ? "H265" : "H264";
    params.video_width=width;params.video_height=height;
    params.video_fps_num=30;params.video_fps_den=1;params.has_audio=false;
    params.pause_at_decode_time=true;params.low_latency=false;
    report(ctx,"SMP Load begin");
    if(!smp_player_load(ctx->player,&params)){report(ctx,"smp_player_load refused");goto fail;}
    report(ctx,"Hardware SMP Load accepted");
    return ctx;
fail:
    if(ctx->player)smp_player_destroy(ctx->player);
    if(ctx->window[0])SDL_webOSDestroyExportedWindow(ctx->window);
    if(ctx->log)fclose(ctx->log);
    free(ctx);return NULL;
}
int luma_hw_feed(void *opaque,const void *packet,size_t bytes,int64_t pts_ns) {
    LumaHW *ctx=opaque;
    if(!ctx||!ctx->player||!packet||bytes==0)return -1;
    smp_feed_result result=smp_player_feed(ctx->player,packet,bytes,pts_ns,SMP_ES_VIDEO);
    if(result==SMP_FEED_OK) {
        ctx->packets++;
        ctx->retry_count=0;
        if(ctx->packets<=6 || ctx->packets%300==0){
            if(ctx->log){fprintf(ctx->log,"[LUMA HARDWARE] accepted video frame %lu pts %.2f s\n",
                ctx->packets,(double)pts_ns/1e9);fflush(ctx->log);}
        }
        return 0;
    }
    if(result==SMP_FEED_BUFFER_FULL || result==SMP_FEED_PENDING){
        ctx->retry_count++;
        if(ctx->retry_count==1 || ctx->retry_count%100==0){
            if(ctx->log){fprintf(ctx->log,"[LUMA HARDWARE] backpressure result %d repeats %lu after %lu accepted frames\n",(int)result,ctx->retry_count,ctx->packets);fflush(ctx->log);}
        }
        return 1;
    }
    report(ctx,"SMP Feed returned error");return -1;
}
int luma_hw_ended(void *opaque) {
    LumaHW *ctx=opaque;
    return ctx && ctx->player ? (smp_player_ended(ctx->player) || smp_player_errored(ctx->player)) : 1;
}
void luma_hw_close(void *opaque) {
    LumaHW *ctx=opaque;if(!ctx)return;
    report(ctx,"Closing hardware plane");
    if(ctx->player){
        smp_player_push_eos(ctx->player);
        smp_player_unload(ctx->player);
        smp_player_destroy(ctx->player);
    }
    if(ctx->window[0])SDL_webOSDestroyExportedWindow(ctx->window);
    if(ctx->log)fclose(ctx->log);
    free(ctx);
}
