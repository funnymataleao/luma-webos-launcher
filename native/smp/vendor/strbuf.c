#include "strbuf.h"

#include <stdarg.h>
#include <stdio.h>

void strbuf_init(strbuf *sb, char *storage, size_t cap) {
    sb->buf = storage;
    sb->cap = cap;
    sb->len = 0;
    sb->truncated = cap == 0;
    if (cap > 0) {
        storage[0] = '\0';
    }
}

void strbuf_addf(strbuf *sb, const char *fmt, ...) {
    if (sb->truncated) {
        return;
    }
    size_t space = sb->cap - sb->len;
    va_list args;
    va_start(args, fmt);
    int written = vsnprintf(sb->buf + sb->len, space, fmt, args);
    va_end(args);

    if (written < 0 || (size_t) written >= space) {
        sb->truncated = true;
        return;
    }
    sb->len += (size_t) written;
}

void strbuf_add_json_string(strbuf *sb, const char *value) {
    strbuf_addf(sb, "\"");
    for (const char *p = value; *p != '\0' && !sb->truncated; p++) {
        unsigned char c = (unsigned char) *p;
        switch (c) {
            case '"':
                strbuf_addf(sb, "\\\"");
                break;
            case '\\':
                strbuf_addf(sb, "\\\\");
                break;
            case '\n':
                strbuf_addf(sb, "\\n");
                break;
            case '\r':
                strbuf_addf(sb, "\\r");
                break;
            case '\t':
                strbuf_addf(sb, "\\t");
                break;
            default:
                if (c < 0x20) {
                    strbuf_addf(sb, "\\u%04x", c);
                } else {
                    strbuf_addf(sb, "%c", c);
                }
                break;
        }
    }
    strbuf_addf(sb, "\"");
}

const char *strbuf_str(const strbuf *sb) {
    return sb->truncated ? NULL : sb->buf;
}
