/**
 * Copyright 2026 Como Technologies, LTD
 * Licensed under the Apache License, Version 2.0
 *
 * Argus event caching and deduplication.
 * Implements an LRU cache for recent events to prevent duplicate notifications.
 */

#ifndef ARGUSCACHE_H
#define ARGUSCACHE_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <stdbool.h>
#include <time.h>

/**
 * Opaque handle for the event cache.
 */
typedef struct argus_cache argus_cache_t;

/**
 * Cache entry representing a recent event.
 */
typedef struct {
    char *path;             /* File path */
    uint32_t mask;          /* Event mask */
    time_t timestamp;       /* Event timestamp */
    uint32_t cookie;        /* inotify cookie (for move events) */
} argus_cache_entry_t;

/**
 * Cache statistics.
 */
typedef struct {
    uint64_t hits;          /* Cache hits (deduplicated events) */
    uint64_t misses;        /* Cache misses (new events) */
    uint64_t evictions;     /* LRU evictions */
    uint32_t size;          /* Current number of entries */
    uint32_t capacity;      /* Maximum capacity */
} argus_cache_stats_t;

/**
 * Create a new event cache.
 *
 * @param capacity     Maximum number of entries
 * @param ttl_seconds  Time-to-live for entries (0 = no expiry)
 * @return Cache handle, or NULL on error
 */
argus_cache_t *argus_cache_create(uint32_t capacity, uint32_t ttl_seconds);

/**
 * Destroy the cache and free all resources.
 *
 * @param cache  Cache handle to destroy
 */
void argus_cache_destroy(argus_cache_t *cache);

/**
 * Check if an event is in the cache (and update if found).
 * Returns true if the event was found (duplicate), false if new.
 *
 * @param cache  Cache handle
 * @param path   File path
 * @param mask   Event mask
 * @param cookie inotify cookie
 * @return true if duplicate, false if new event
 */
bool argus_cache_check(argus_cache_t *cache, const char *path, uint32_t mask, uint32_t cookie);

/**
 * Add an event to the cache.
 * If the cache is full, the oldest entry is evicted.
 *
 * @param cache  Cache handle
 * @param path   File path
 * @param mask   Event mask
 * @param cookie inotify cookie
 * @return 0 on success, -1 on error
 */
int argus_cache_add(argus_cache_t *cache, const char *path, uint32_t mask, uint32_t cookie);

/**
 * Remove an entry from the cache.
 *
 * @param cache  Cache handle
 * @param path   File path
 * @return 0 on success, -1 if not found
 */
int argus_cache_remove(argus_cache_t *cache, const char *path);

/**
 * Clear all entries from the cache.
 *
 * @param cache  Cache handle
 */
void argus_cache_clear(argus_cache_t *cache);

/**
 * Expire old entries based on TTL.
 *
 * @param cache  Cache handle
 * @return Number of entries expired
 */
int argus_cache_expire(argus_cache_t *cache);

/**
 * Get cache statistics.
 *
 * @param cache  Cache handle
 * @param stats  Output statistics structure
 */
void argus_cache_get_stats(argus_cache_t *cache, argus_cache_stats_t *stats);

/**
 * Reset cache statistics counters.
 *
 * @param cache  Cache handle
 */
void argus_cache_reset_stats(argus_cache_t *cache);

#ifdef __cplusplus
}
#endif

#endif /* ARGUSCACHE_H */
