// Lightweight online demux. The original H.264 bitstream is passed through,
// without decoding, scaling, re-encoding, or storing any video file.
// Stdout protocol: "LVID" magic once; then little-endian length u32,
// LVID (HEVC, unchanged): length u32, decode/display clock ns u64, payload.
 // LVI2 (H.264): length u32, decode scheduling ns u64, actual display PTS
 // ns u64, payload. This fixes B-frame ordering without changing Amazon.
#include <gst/gst.h>
#include <gst/app/gstappsink.h>
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>
static volatile sig_atomic_t ending=0;
static void sigterm(int sig){(void)sig;ending=1;}
static int write_bytes(const void *ptr,size_t count){
    return fwrite(ptr,1,count,stdout)==count;
}
static int write_header(uint32_t length,uint64_t pts,uint64_t presentation,bool separate_pts){
    unsigned char h[12];
    for(int i=0;i<4;i++)h[i]=(unsigned char)(length>>(i*8));
    for(int i=0;i<8;i++)h[i+4]=(unsigned char)(pts>>(i*8));
    if(!write_bytes(h,sizeof(h)))return 0;
    if(separate_pts){
        unsigned char extra[8];
        for(int i=0;i<8;i++)extra[i]=(unsigned char)(presentation>>(i*8));
        return write_bytes(extra,sizeof(extra));
    }
    return 1;
}
int main(int argc,char **argv){
    if(argc!=3 || (strcmp(argv[1],"h264") && strcmp(argv[1],"h265")) || strncmp(argv[2],"https://",8)!=0 || strlen(argv[2])>2048){
        fprintf(stderr,"Usage: luma-demux h264|h265 HTTPS_VIDEO_URL\n");return 2;
    }
    signal(SIGTERM,sigterm);signal(SIGPIPE,SIG_IGN);
    gst_init(NULL,NULL);
    GError *error=NULL;
    const char *desc=strcmp(argv[1],"h265")==0?
        "souphttpsrc name=source ! qtdemux name=d d.video_0 ! queue max-size-buffers=6 ! "
        "h265parse config-interval=-1 ! video/x-h265,stream-format=byte-stream,alignment=au ! "
        "appsink name=output sync=false emit-signals=false max-buffers=6 drop=false" :

        "souphttpsrc name=source ! qtdemux name=d d.video_0 ! queue max-size-buffers=6 ! "
        "h264parse config-interval=-1 ! "
        "video/x-h264,stream-format=byte-stream,alignment=au ! "
        "appsink name=output sync=false emit-signals=false max-buffers=6 drop=false";
    GstElement *pipe=gst_parse_launch(desc,&error);
    if(!pipe){fprintf(stderr,"Parse failed: %s\n",error?error->message:"unknown");return 3;}
    GstElement *src=gst_bin_get_by_name(GST_BIN(pipe),"source");
    GstElement *sink=gst_bin_get_by_name(GST_BIN(pipe),"output");
    if(!src || !sink){fprintf(stderr,"Missing source/sink\n");return 4;}
    g_object_set(src,"location",argv[2],NULL);
    GstBus *bus=gst_element_get_bus(pipe);
    gst_element_set_state(pipe,GST_STATE_PLAYING);
    int started=0;
    uint64_t decode_clock=0,frame=0;
    bool separate_pts=strcmp(argv[1],"h264")==0;
    GstClockTime timeline_origin=GST_CLOCK_TIME_NONE;
    uint64_t last_presentation=0,first_frame_ns=0;
    while(!ending){
        GstSample *sample=gst_app_sink_try_pull_sample(GST_APP_SINK(sink),GST_SECOND/2);
        if(!sample){
            GstMessage *m=gst_bus_pop_filtered(bus,GST_MESSAGE_ERROR|GST_MESSAGE_EOS);
            if(m){
                if(GST_MESSAGE_TYPE(m)==GST_MESSAGE_ERROR){
                    GError *err=NULL;gchar *debug=NULL;
                    gst_message_parse_error(m,&err,&debug);
                    fprintf(stderr,"Video demux error: %s (%s)\n",
                        err?err->message:"unknown",debug?debug:"no details");
                    if(err)g_error_free(err);g_free(debug);
                } else fprintf(stderr,"Video reached EOS after %llu frames\n",(unsigned long long)frame);
                gst_message_unref(m);break;
            }
            continue;
        }
        GstBuffer *buf=gst_sample_get_buffer(sample);
        GstMapInfo data;
        if(gst_buffer_map(buf,&data,GST_MAP_READ)){
            if(data.size>0 && data.size<2*1024*1024){
                if(!started){
                    if(!write_bytes(separate_pts?"LVI2":"LVID",4))ending=1;
                    started=1;
                }
                // BUFFERSTREAM consumes access units in decode order. MP4 PTS
                // are not monotonic when the video has B-frames (0,.067,.033),
                // so schedule using the access-unit decode clock instead.
                // The original compressed video bytes remain unchanged.
                uint64_t pts=decode_clock;
                uint64_t duration=GST_BUFFER_DURATION_IS_VALID(buf)
                    ? GST_BUFFER_DURATION(buf) : GST_SECOND/30;
                if(duration<10*GST_MSECOND || duration>100*GST_MSECOND)
                    duration=GST_SECOND/30;
                if(separate_pts && getenv("LUMA_H264_FIXED_30"))
                    duration=GST_SECOND/30;
                decode_clock += duration;
                uint64_t presentation=pts;
                if(separate_pts){
                    // GStreamer h264parse produces decode-ordered access units.
                    // PTS is intentionally *non-monotonic* for B-frames. NDL
                    // must see the true PTS; throttling the pipe uses DTS.
                    const GstClockTime dts=GST_BUFFER_DTS_IS_VALID(buf)?
                        GST_BUFFER_DTS(buf):GST_CLOCK_TIME_NONE;
                    const GstClockTime real_pts=GST_BUFFER_PTS_IS_VALID(buf)?
                        GST_BUFFER_PTS(buf):GST_CLOCK_TIME_NONE;
                    if(timeline_origin==GST_CLOCK_TIME_NONE){
                        timeline_origin=dts!=GST_CLOCK_TIME_NONE?dts:
                            (real_pts!=GST_CLOCK_TIME_NONE?real_pts:0);
                    }
                    if(real_pts!=GST_CLOCK_TIME_NONE && real_pts>=timeline_origin)
                        presentation=real_pts-timeline_origin;
                    else if(real_pts!=GST_CLOCK_TIME_NONE){
                        // Clip-start reorder can produce negative display times;
                        // clamp only such leading preroll, not the normal B-frames.
                        presentation=0;
                    } else {
                        presentation=pts;
                    }
                    if(frame<9 || frame%300==0){
                        fprintf(stderr,"H264 AU %llu decode=%.2fms display=%.2fms dts=%llu pts=%llu\n",
                            (unsigned long long)frame,(double)pts/1e6,
                            (double)presentation/1e6,
                            (unsigned long long)dts,(unsigned long long)real_pts);
                    }
                }else if(frame==0){
                    fprintf(stderr,"HEVC legacy pacing unchanged, AU=%.2fms\n",
                        (double)duration/1e6);
                }
                if(!write_header((uint32_t)data.size,pts,presentation,separate_pts)
                    || !write_bytes(data.data,data.size)){
                    ending=1;
                }
                frame++;
                if(frame==1 || frame%300==0)
                    fprintf(stderr,"Demuxed %llu original access units (no decode/reencode)\n",
                        (unsigned long long)frame);
            }
            gst_buffer_unmap(buf,&data);
        }
        gst_sample_unref(sample);
    }
    if(started)fflush(stdout);
    gst_element_set_state(pipe,GST_STATE_NULL);
    gst_object_unref(bus);gst_object_unref(src);gst_object_unref(sink);gst_object_unref(pipe);
    return started?0:5;
}
