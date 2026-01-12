/**
 * Copyright 2026 Como Technologies, LTD
 * Licensed under the Apache License, Version 2.0
 *
 * Argus inotify wrapper for file integrity monitoring.
 * This is the core C library that interfaces directly with the Linux inotify API.
 */

#ifndef ARGUSNOTIFY_H
#define ARGUSNOTIFY_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <stdbool.h>
#include <sys/inotify.h>

/* Maximum events to process in one epoll iteration */
#define EPOLL_MAX_EVENTS 64

/* Signal used to stop a watcher */
#define ARGUSNOTIFY_KILL SIGKILL

/**
 * Callback function type for log events.
 * Called when an inotify event is detected.
 */
typedef void (*arguswatch_logfn)(
    const char *name,           /* Watcher name */
    const char *nodename,       /* Node name */
    const char *podname,        /* Pod name */
    const char *event,          /* Event type (modify, create, etc.) */
    const char *path,           /* Full path of the file */
    const char *filename,       /* Filename portion */
    bool isdir,                 /* Whether path is a directory */
    const char *tags            /* JSON tags string */
);

/**
 * Watch configuration flags.
 */
#define ARGUS_FLAG_RECURSIVE    (1 << 0)
#define ARGUS_FLAG_ONLYDIR      (1 << 1)
#define ARGUS_FLAG_FOLLOWMOVE   (1 << 2)

/**
 * Start an inotify watcher for the specified paths.
 *
 * @param name      Watcher name (from ArgusWatcher CR)
 * @param nodename  Kubernetes node name
 * @param podname   Pod name being watched
 * @param pid       Container init PID
 * @param sid       Session ID for tracking
 * @param pathc     Number of paths to watch
 * @param paths     Array of paths to watch
 * @param ignorec   Number of ignore patterns
 * @param ignores   Array of glob patterns to ignore
 * @param mask      inotify event mask
 * @param flags     Watch flags (ARGUS_FLAG_*)
 * @param maxdepth  Maximum recursion depth (0 = unlimited)
 * @param tags      JSON string of tags
 * @param logformat Custom log format template (or NULL for default)
 * @param logfn     Callback function for log events
 *
 * @return 0 on success, -1 on error
 */
int start_inotify_watcher(
    const char *name,
    const char *nodename,
    const char *podname,
    int pid,
    int sid,
    unsigned int pathc,
    const char *paths[],
    unsigned int ignorec,
    const char *ignores[],
    uint32_t mask,
    uint32_t flags,
    int maxdepth,
    const char *tags,
    const char *logformat,
    arguswatch_logfn logfn
);

/**
 * Stop a watcher by sending a kill signal.
 *
 * @param pid  Process ID of the watcher to stop
 */
void send_watcher_kill_signal(int pid);

/**
 * Convert event name string to inotify mask.
 *
 * @param event  Event name (e.g., "modify", "create", "all")
 * @return inotify mask value, or 0 if unknown
 */
uint32_t event_name_to_mask(const char *event);

/**
 * Convert inotify mask to event name string.
 *
 * @param mask  inotify event mask
 * @return Event name string (statically allocated)
 */
const char *mask_to_event_name(uint32_t mask);

/**
 * Get the number of active watch descriptors.
 *
 * @return Number of active watch descriptors
 */
int get_watch_descriptor_count(void);

#ifdef __cplusplus
}
#endif

#endif /* ARGUSNOTIFY_H */
