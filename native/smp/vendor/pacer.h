/*
 * A monotonic clock and a sleep-until, so the samples feed in real time.
 *
 * Nothing paces the pipeline for us: Feed() accepts data as fast as its buffers allow, and
 * answers "BufferFull" once they do not. Feeding a ten second file as fast as the disk
 * allows would just bounce off that back-pressure, so the samples hold each access unit
 * until its presentation time has arrived.
 */
#pragma once

#include <stdint.h>

/* CLOCK_MONOTONIC in nanoseconds. The same unit the Feed() payload's "pts" uses, which is
 * why the samples keep everything in nanoseconds end to end. */
int64_t pacer_now_ns(void);

void pacer_sleep_ns(int64_t duration_ns);

/* Milliseconds since the first call, which the samples make at startup. Used to timestamp
 * the log so it is obvious where the wait before playback actually goes - pipeline
 * creation, resource acquisition, or decoding the first picture. */
double pacer_uptime_ms(void);

/* Sleeps until `start_ns + pts_ns`. Returns immediately if that moment has passed, so a
 * sample that falls behind catches up rather than accumulating delay. */
void pacer_sleep_until(int64_t start_ns, int64_t pts_ns);
