/*
 * A bounded printf accumulator, used to build the pipeline's JSON payloads.
 *
 * The payloads the samples send are fixed in shape - the only things that vary are a
 * handful of numbers and two or three identifiers. Building them with a JSON DOM library
 * (webOS ships pbnjson) would hide the one thing a sample exists to show: the exact bytes
 * that go over to libplayerAPIs. So the payload builders are printf calls, and this is the
 * two-hundred-line safety net that keeps them from overrunning a stack buffer.
 *
 * Everything writes into caller-provided storage; there is no allocation.
 */
#pragma once

#include <stdbool.h>
#include <stddef.h>

typedef struct strbuf {
    char *buf;
    size_t cap;
    size_t len;
    bool truncated;
} strbuf;

void strbuf_init(strbuf *sb, char *storage, size_t cap);

void strbuf_addf(strbuf *sb, const char *fmt, ...) __attribute__((format(printf, 2, 3)));

/* Appends `value` as a quoted, escaped JSON string. Identifiers coming from getenv() or
 * from SDL are not attacker-controlled here, but a sample that splices raw text into JSON
 * is a bad thing to copy. */
void strbuf_add_json_string(strbuf *sb, const char *value);

/* NUL-terminated contents, or NULL if anything was truncated - callers should treat a
 * truncated payload as a hard error rather than send half a JSON document. */
const char *strbuf_str(const strbuf *sb);
