/*
 * The one C++ file in this repo. See smp_api.h for why it exists.
 *
 * Three compile-time knobs pick the generation, all set from the sample's CMakeLists:
 *
 *   SMP_CTOR_NO_UID        webOS 1 only exports StarfishMediaAPIs(), not
 *                          StarfishMediaAPIs(const char *)
 *   SMP_LOAD_HAS_USERDATA  webOS 3+ export a Load() overload that carries a user pointer
 *                          through to the callback; webOS 1 and 2 do not
 *   _GLIBCXX_USE_CXX11_ABI=0
 *                          webOS 3 and older were built before the std::string ABI change,
 *                          so Feed() is mangled ...4FeedEPKc there and ...4FeedB5cxx11EPKc
 *                          from webOS 4 on. The compiler picks the right mangling for us
 *                          once the macro is set; nothing here has to name it.
 */
#include "smp_api.h"

#include <cstdio>
#include <cstring>
#include <new>
#include <string>

#include <StarfishMediaAPIs.h>

#ifndef SMP_CTOR_NO_UID
#define SMP_CTOR_NO_UID 0
#endif
#ifndef SMP_LOAD_HAS_USERDATA
#define SMP_LOAD_HAS_USERDATA 1
#endif

/*
 * Methods that do not exist on every generation are called through a weak reference to
 * their mangled name instead of as a method. A weak undefined symbol resolves to null when
 * the TV's libplayerAPIs does not export it, so the app still loads and we can degrade;
 * calling it as a method would abort the process at startup instead.
 *
 * (The sysroot's libplayerAPIs.so is a link-time stub that exports every symbol at one
 * address, so this cannot be caught at build time - only at runtime, on the device.)
 */
extern "C" bool
_ZN17StarfishMediaAPIs16notifyForegroundEv(StarfishMediaAPIs *api) __attribute__((weak));

#if SMP_CTOR_NO_UID
extern "C" void
_ZN17StarfishMediaAPIsC1Ev(StarfishMediaAPIs *api) __attribute__((weak));
#endif

namespace {

constexpr size_t kFeedReplyMax = 128;

struct smp_api_impl {
    StarfishMediaAPIs *inner;
    smp_event_fn *callback;
    void *user;
    char last_feed_reply[kFeedReplyMax];
};

#if !SMP_LOAD_HAS_USERDATA
/*
 * webOS 1 and 2 only export Load(payload, void(*)(int, int64_t, const char *)) - there is
 * nowhere to thread a context pointer through, so the callback has to find it in a global.
 * These samples create exactly one player, which is what makes that acceptable; a real app
 * targeting those generations needs a registry keyed on something it can recover.
 */
smp_api_impl *g_only_player = nullptr;

void trampoline(int type, int64_t num_value, const char *str_value) {
    smp_api_impl *impl = g_only_player;
    if (impl != nullptr && impl->callback != nullptr) {
        impl->callback(type, num_value, str_value, impl->user);
    }
}
#endif

}// namespace

extern "C" {

struct smp_api {
    smp_api_impl impl;
};

smp_api *smp_api_create(const char *uid) {
    smp_api *api = new (std::nothrow) smp_api();
    if (api == nullptr) {
        return nullptr;
    }
    std::memset(&api->impl, 0, sizeof(api->impl));

#if SMP_CTOR_NO_UID
    (void) uid;
    if (_ZN17StarfishMediaAPIsC1Ev == nullptr) {
        std::fprintf(stderr, "smp: this firmware has no StarfishMediaAPIs() constructor\n");
        delete api;
        return nullptr;
    }
    /* The header's StarfishMediaAPIs is deliberately over-sized (it ends in a 4 KiB
     * padding member) precisely so raw storage of sizeof() is safe here. */
    void *storage = ::operator new(sizeof(StarfishMediaAPIs), std::nothrow);
    if (storage == nullptr) {
        delete api;
        return nullptr;
    }
    _ZN17StarfishMediaAPIsC1Ev(static_cast<StarfishMediaAPIs *>(storage));
    api->impl.inner = static_cast<StarfishMediaAPIs *>(storage);
#else
    api->impl.inner = new (std::nothrow) StarfishMediaAPIs(uid);
    if (api->impl.inner == nullptr) {
        delete api;
        return nullptr;
    }
#endif

#if !SMP_LOAD_HAS_USERDATA
    g_only_player = &api->impl;
#endif
    return api;
}

void smp_api_destroy(smp_api *api) {
    if (api == nullptr) {
        return;
    }
#if !SMP_LOAD_HAS_USERDATA
    if (g_only_player == &api->impl) {
        g_only_player = nullptr;
    }
#endif
    delete api->impl.inner;
    delete api;
}

bool smp_api_load(smp_api *api, const char *payload, smp_event_fn *callback, void *user) {
    api->impl.callback = callback;
    api->impl.user = user;
#if SMP_LOAD_HAS_USERDATA
    return api->impl.inner->Load(payload, callback, user);
#else
    return api->impl.inner->Load(payload, trampoline);
#endif
}

bool smp_api_play(smp_api *api) { return api->impl.inner->Play(); }

bool smp_api_pause(smp_api *api) { return api->impl.inner->Pause(); }

bool smp_api_push_eos(smp_api *api) { return api->impl.inner->pushEOS(); }

bool smp_api_unload(smp_api *api) { return api->impl.inner->Unload(); }

bool smp_api_notify_foreground(smp_api *api) {
    if (_ZN17StarfishMediaAPIs16notifyForegroundEv == nullptr) {
        return false;
    }
    return _ZN17StarfishMediaAPIs16notifyForegroundEv(api->impl.inner);
}

const char *smp_api_media_id(smp_api *api) { return api->impl.inner->getMediaID(); }

smp_feed_result smp_api_feed(smp_api *api, const char *payload) {
    /* This is the ABI-sensitive call: the return is a std::string built by the TV's
     * libstdc++ and destroyed by ours, so the two have to agree on its layout. Getting
     * _GLIBCXX_USE_CXX11_ABI wrong for the target generation shows up here, as a crash. */
    std::string reply = api->impl.inner->Feed(payload);

    std::snprintf(api->impl.last_feed_reply, kFeedReplyMax, "%s", reply.c_str());

    if (reply.find("Ok") != std::string::npos) {
        return SMP_FEED_OK;
    }
    if (reply.find("BufferFull") != std::string::npos) {
        return SMP_FEED_BUFFER_FULL;
    }
    if (reply.find("Pending") != std::string::npos) {
        return SMP_FEED_PENDING;
    }
    return SMP_FEED_ERROR;
}

const char *smp_api_last_feed_reply(const smp_api *api) { return api->impl.last_feed_reply; }
}
