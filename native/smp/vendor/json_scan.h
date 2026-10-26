/*
 * Pulling one number out of a JSON document, without a JSON parser.
 *
 * The pipeline reports things like {"video":{"width":1280,"height":720,...}} through the
 * Load callback, and the samples want one or two of those numbers. webOS does ship pbnjson,
 * but its API differs enough between versions that homebrew players carry a dlsym shim for
 * it - not something a sample should demonstrate.
 *
 * So this is a deliberate shortcut, and it is only safe because of what it is used on:
 * well-formed machine-generated payloads where the key of interest appears once. It is not
 * a JSON parser, and it should not grow into one - if a sample ever needs real parsing,
 * link pbnjson and pay for it there.
 */
#pragma once

#include <stdbool.h>

/* Finds "<key>": <integer> anywhere in `json`. Returns false if the key is absent or is
 * not followed by an integer. */
bool json_scan_int(const char *json, const char *key, int *out_value);
