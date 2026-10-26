// Hardware playback through the version-stable uMediaServer C facade.
// On the C1 the C++ uMediaClient constructor uses the *two-argument* ABI.
// The SDK's three-argument constructor is not exported on retail firmware.
// Never instantiate the C++ class directly; C facade owns it and the event loop.
#include <SDL.h>
#include <SDL_system.h>
#include <uMediaClient_wrapper.h>
#include <atomic>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <dlfcn.h>
#include <mutex>
#include <string>
#include <thread>
#include <unistd.h>

using Clock=std::chrono::steady_clock;
using SelectTrack=bool (*)(void*,std::string&,int32_t);
struct Cinema {
 std::mutex mu;
 void *client=nullptr;
 std::string window;
 std::atomic<int> state{0};
 std::atomic<int64_t> position{0}, duration{0};
 std::atomic<bool> shutting_down{false};
 Clock::time_point play_started{Clock::now()};
 Clock::time_point replace_at{Clock::time_point::max()};
 Clock::time_point loaded_at{Clock::now()};
 size_t replaced=0;
 int64_t base_position=0;
 std::thread waiter;
 Cinema() {
  client=reinterpret_cast<void*>(uMediaClientCreate());
  if(!client)client=reinterpret_cast<void*>(uMediaClientCreatePrivate());
  if(client)waiter=std::thread([this]{loop();});
 }
 ~Cinema() {
  shutting_down.store(true);
  if(waiter.joinable())waiter.join();
  std::lock_guard<std::mutex> guard(mu);
  if(client){uMediaClientUnload(client);uMediaClientDestroy(client);client=nullptr;}
  if(!window.empty()){SDL_webOSDestroyExportedWindow(window.c_str());window.clear();}
 }
 void loop() {
  while(!shutting_down.load()){
   if(state.load()==1){
    std::lock_guard<std::mutex> guard(mu);
    if(client && state.load()==1 && uMediaClientPlay(client)){
     play_started=Clock::now();
     state.store(2);
    }
   }
   // Re-apply the placement once the pipeline exists; the compositor may
   // only bind the exported surface after the media connects to it. Slow
   // streams (20 s HLS segments) connect late, so keep re-placing for a while.
   if(Clock::now()>=replace_at){
    static const int after_ms[]={1500,3000,5000,8000,12000,18000,26000};
    std::lock_guard<std::mutex> guard(mu);
    place();
    replaced++;
    replace_at=replaced<sizeof after_ms/sizeof *after_ms
     ?loaded_at+std::chrono::milliseconds(after_ms[replaced])
     :Clock::time_point::max();
   }
   usleep(150000);
  }
 }
 // Diagnostic trail next to Luma's other logs. Never receives URLs.
 static void note(const char *line){
  if(FILE *f=fopen("/media/developer/com.denisefremov.luma/luma-cinema.log","a")){fprintf(f,"native %s\n",line);fclose(f);}
 }
 // How the video surface is placed; a file switch lets us compare on the TV
 // without a rebuild: both (default), crop, set (2.2.81 behaviour), none.
 // Crop alone kept full-frame 1920x1080 video black (letterboxed and 720p
 // sources showed); setting the exported window as well shows every size.
 static std::string mode(){
  std::string m="both";
  if(FILE *f=fopen("/media/developer/com.denisefremov.luma/cinema-window","r")){
   char buf[16]={0};
   if(fgets(buf,sizeof buf,f)){m=buf;while(!m.empty()&&(m.back()=='\n'||m.back()==' '))m.pop_back();}
   fclose(f);
  }
  return m;
 }
 std::string placement="both";
 // Video frame size from the stream's playlist; 0 when unknown.
 int video_w=0,video_h=0;
 // The largest rect of the video's aspect inside w x h, centred. Unknown
 // sizes fill the whole area, as 16:9 frames do.
 SDL_Rect fit(int w,int h) const {
  if(video_w<=0||video_h<=0)return SDL_Rect{0,0,w,h};
  int fw=w,fh=static_cast<int>(static_cast<int64_t>(w)*video_h/video_w);
  if(fh>h){fh=h;fw=static_cast<int>(static_cast<int64_t>(h)*video_w/video_h);}
  return SDL_Rect{(w-fw)/2,(h-fh)/2,fw,fh};
 }
 // Map the frame onto the Luma window, like Kodi's set_crop_region;
 // destination is in the window's own coordinates.
 void place(){
  if(window.empty())return;
  int ww=1920,wh=1080;
  if(SDL_Window *w=SDL_GetWindowFromID(1))SDL_GetWindowSize(w,&ww,&wh);
  char line[160];
  if(placement=="set"||placement=="both"){
   int pw=1920,ph=1080;
   SDL_webOSGetPanelResolution(&pw,&ph);
   SDL_Rect source={0,0,pw,ph},dest=fit(pw,ph);
   bool ok=SDL_webOSSetExportedWindow(window.c_str(),&source,&dest);
   snprintf(line,sizeof line,"set-exported panel=%dx%d dest=%dx%d ok=%d",pw,ph,dest.w,dest.h,ok);note(line);
  }
  if(placement=="crop"||placement=="both"){
   int vw=video_w>0?video_w:1920,vh=video_h>0?video_h:1080;
   SDL_Rect org={0,0,vw,vh},src={0,0,vw,vh},dst=fit(ww,wh);
   bool ok=SDL_webOSExportedSetCropRegion(window.c_str(),&org,&src,&dst);
   snprintf(line,sizeof line,"crop video=%dx%d window=%dx%d ok=%d",vw,vh,ww,wh,ok);note(line);
  }
 }
 int open(const char *uri) {
  if(!client || !uri || !uri[0])return 0;
  std::lock_guard<std::mutex> guard(mu);
  state.store(0);
  uMediaClientUnload(client);
  if(!window.empty()){SDL_webOSDestroyExportedWindow(window.c_str());window.clear();}
  placement=mode();
  if(placement!="none"){
   const char *id=SDL_webOSCreateExportedWindow(SDL_WEBOS_EXPORTED_WINDOW_TYPE_VIDEO);
   if(id)window=id;
  }
  place();
  std::string payload="{\"option\":{\"appId\":\"com.denisefremov.luma\"";
  if(!window.empty())payload+=",\"windowId\":\""+window+"\"";
  payload+="}}";
  state.store(1);position.store(0);duration.store(0);base_position=0;
  int answer=uMediaClientLoad(client,const_cast<char*>(uri),kMedia,payload.data());
  if(!answer)state.store(-1);
  char line[160];
  snprintf(line,sizeof line,"load=%d mode=%s window=%s",answer,placement.c_str(),window.empty()?"-":window.c_str());
  note(line);
  loaded_at=Clock::now();
  replaced=0;
  replace_at=loaded_at+std::chrono::milliseconds(1500);
  return answer?1:0;
 }
 int control(int command,int64_t value){
  std::lock_guard<std::mutex> guard(mu);
  if(!client)return 0;
  switch(command){
   case 0:if(uMediaClientPlay(client)){play_started=Clock::now();state.store(2);return 1;}return 0;
   case 1:if(uMediaClientPause(client)){if(state.load()==2)base_position=pos();state.store(3);return 1;}return 0;
   case 2:note("seek");if(uMediaClientSeek(client,static_cast<long>(value))){base_position=value;position.store(value);play_started=Clock::now();return 1;}return 0;
   case 3:case 4:{
    auto f=reinterpret_cast<SelectTrack>(dlsym(RTLD_DEFAULT,"_ZN12uMediaServer12uMediaClient11selectTrackERNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEEi"));
    if(!f)return 0;
    std::string type=command==3?"audio":"subtitle";
    bool ok=f(client,type,static_cast<int32_t>(value));
    char line[96];
    snprintf(line,sizeof line,"selectTrack %s %d ok=%d",type.c_str(),static_cast<int>(value),ok);
    note(line);
    return ok?1:0;
   }
   case 5:state.store(0);return uMediaClientUnload(client)?1:0;
  }
  return 0;
 }
 int64_t pos()const {
  if(state.load()!=2)return base_position;
  return base_position+std::chrono::duration_cast<std::chrono::milliseconds>(Clock::now()-play_started).count();
 }
};
extern "C" {
void *luma_cinema_create(){try{return new Cinema();}catch(...){return nullptr;}}
void luma_cinema_destroy(void *p){delete static_cast<Cinema*>(p);}
void luma_cinema_video_size(void *p,int w,int h){if(auto *c=static_cast<Cinema*>(p)){std::lock_guard<std::mutex> guard(c->mu);c->video_w=w;c->video_h=h;}}
int luma_cinema_open(void *p,const char *uri){return p?static_cast<Cinema*>(p)->open(uri):0;}
int luma_cinema_control(void *p,int cmd,int64_t value){return p?static_cast<Cinema*>(p)->control(cmd,value):0;}
int luma_cinema_state(void *p){return p?static_cast<Cinema*>(p)->state.load():-1;}
int64_t luma_cinema_position(void *p){return p?static_cast<Cinema*>(p)->pos():0;}
int64_t luma_cinema_duration(void *p){return p?static_cast<Cinema*>(p)->duration.load():0;}
// The C ABI exposes no video-frame callback. Do not invent first-frame timestamps.
int64_t luma_cinema_first_ms(void *){return -1;}

// Exported video surface helpers for the LS2 (com.webos.media) path.
// The window id is copied by the caller; destroy with the same id.
const char *luma_cinema_window_create(){
 return SDL_webOSCreateExportedWindow(SDL_WEBOS_EXPORTED_WINDOW_TYPE_VIDEO);
}
// Map the whole video frame (vw x vh) onto the whole Luma window, as Kodi's
// set_crop_region does. Destination is in the parent surface's coordinates.
int luma_cinema_window_fit(const char *id,int vw,int vh){
 if(!id||!id[0]||vw<=0||vh<=0)return 0;
 int ww=1920,wh=1080;
 if(SDL_Window *w=SDL_GetWindowFromID(1))SDL_GetWindowSize(w,&ww,&wh);
 SDL_Rect org={0,0,vw,vh},src={0,0,vw,vh},dst={0,0,ww,wh};
 return SDL_webOSExportedSetCropRegion(id,&org,&src,&dst)?1:0;
}
void luma_cinema_window_destroy(const char *id){if(id&&id[0])SDL_webOSDestroyExportedWindow(id);}
}
