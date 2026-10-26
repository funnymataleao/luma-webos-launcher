#include "pacer.h"

#include <errno.h>
#include <time.h>

int64_t pacer_now_ns(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (int64_t) now.tv_sec * 1000000000LL + now.tv_nsec;
}

double pacer_uptime_ms(void) {
    static int64_t origin = 0;
    int64_t now = pacer_now_ns();
    if (origin == 0) {
        origin = now;
    }
    return (double) (now - origin) / 1e6;
}

void pacer_sleep_ns(int64_t duration_ns) {
    if (duration_ns <= 0) {
        return;
    }
    struct timespec req = {
            .tv_sec = (time_t) (duration_ns / 1000000000LL),
            .tv_nsec = (long) (duration_ns % 1000000000LL),
    };
    struct timespec rem;
    while (nanosleep(&req, &rem) != 0 && errno == EINTR) {
        req = rem;
    }
}

void pacer_sleep_until(int64_t start_ns, int64_t pts_ns) {
    pacer_sleep_ns(start_ns + pts_ns - pacer_now_ns());
}
