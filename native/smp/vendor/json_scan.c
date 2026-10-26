#include "json_scan.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

bool json_scan_int(const char *json, const char *key, int *out_value) {
    if (json == NULL || key == NULL) {
        return false;
    }

    char needle[64];
    int needle_len = snprintf(needle, sizeof(needle), "\"%s\"", key);
    if (needle_len <= 0 || (size_t) needle_len >= sizeof(needle)) {
        return false;
    }

    const char *found = strstr(json, needle);
    if (found == NULL) {
        return false;
    }

    const char *cursor = found + needle_len;
    while (isspace((unsigned char) *cursor)) {
        cursor++;
    }
    if (*cursor != ':') {
        return false;
    }
    cursor++;
    while (isspace((unsigned char) *cursor)) {
        cursor++;
    }

    char *end = NULL;
    long value = strtol(cursor, &end, 10);
    if (end == cursor) {
        return false;
    }
    *out_value = (int) value;
    return true;
}
