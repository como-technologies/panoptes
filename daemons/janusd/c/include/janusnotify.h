/**
 * Copyright 2026 Como Technologies, LTD
 * Licensed under the Apache License, Version 2.0
 *
 * Janus fanotify wrapper for file access auditing.
 * Core C library interfacing directly with the Linux fanotify API.
 */

#ifndef JANUSNOTIFY_H
#define JANUSNOTIFY_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <stdbool.h>
#include <sys/fanotify.h>

/* Maximum events to process in one epoll iteration */
#define EPOLL_MAX_EVENTS 64

/* Signal used to stop a guard */
#define JANUSNOTIFY_KILL SIGKILL

/**
 * Access response type.
 */
typedef enum {
    JANUS_ALLOW = 0,    /* Allow access */
    JANUS_DENY = 1,     /* Deny access */
    JANUS_AUDIT = 2     /* Allow but audit */
} janus_response_t;

/**
 * Callback function type for access events.
 * Called when a fanotify access event is detected.
 * Returns the access response (allow/deny/audit).
 */
typedef janus_response_t (*janusguard_callback)(
    const char *name,           /* Guard name */
    const char *nodename,       /* Node name */
    const char *podname,        /* Pod name */
    const char *event,          /* Event type (open, access) */
    const char *path,           /* Full path of the file */
    const char *filename,       /* Filename portion */
    pid_t pid,                  /* Process ID requesting access */
    uid_t uid,                  /* User ID */
    bool isdir,                 /* Whether path is a directory */
    const char *tags            /* JSON tags string */
);

/**
 * Callback function for log events (non-permission events).
 */
typedef void (*janusguard_logfn)(
    const char *name,           /* Guard name */
    const char *nodename,       /* Node name */
    const char *podname,        /* Pod name */
    const char *event,          /* Event type */
    const char *path,           /* Full path */
    const char *filename,       /* Filename */
    pid_t pid,                  /* Process ID */
    uid_t uid,                  /* User ID */
    bool isdir,                 /* Is directory */
    janus_response_t response,  /* Allow/deny response */
    const char *tags            /* JSON tags */
);

/**
 * Guard configuration flags.
 */
#define JANUS_FLAG_PERMISSION   (1 << 0)  /* Enable permission events (blocking) */
#define JANUS_FLAG_AUDIT        (1 << 1)  /* Write to kernel audit log */
#define JANUS_FLAG_ONLYDIR      (1 << 2)  /* Only watch directories */
#define JANUS_FLAG_AUTOALLOW    (1 << 3)  /* Auto-allow file owner */

/**
 * Guard mode.
 */
typedef enum {
    JANUS_MODE_AUDIT = 0,       /* Audit only, no blocking */
    JANUS_MODE_ENFORCE = 1      /* Enforce allow/deny rules */
} janus_mode_t;

/**
 * Start a fanotify guard for the specified paths.
 *
 * @param name       Guard name (from JanusGuard CR)
 * @param nodename   Kubernetes node name
 * @param podname    Pod name being guarded
 * @param pid        Container init PID
 * @param sid        Session ID for tracking
 * @param allowc     Number of allow patterns
 * @param allows     Array of path patterns to allow
 * @param denyc      Number of deny patterns
 * @param denys      Array of path patterns to deny
 * @param mask       fanotify event mask
 * @param flags      Guard flags (JANUS_FLAG_*)
 * @param mode       Guard mode (audit or enforce)
 * @param tags       JSON string of tags
 * @param logformat  Custom log format template (or NULL)
 * @param callback   Callback for permission decisions
 * @param logfn      Callback for logging events
 *
 * @return 0 on success, -1 on error
 */
int start_fanotify_guard(
    const char *name,
    const char *nodename,
    const char *podname,
    int pid,
    int sid,
    unsigned int allowc,
    const char *allows[],
    unsigned int denyc,
    const char *denys[],
    uint64_t mask,
    uint32_t flags,
    janus_mode_t mode,
    const char *tags,
    const char *logformat,
    janusguard_callback callback,
    janusguard_logfn logfn
);

/**
 * Stop a guard by sending a kill signal.
 *
 * @param pid  Process ID of the guard to stop
 */
void send_guard_kill_signal(int pid);

/**
 * Convert event name string to fanotify mask.
 *
 * @param event  Event name (e.g., "open", "access", "all")
 * @return fanotify mask value, or 0 if unknown
 */
uint64_t janus_event_name_to_mask(const char *event);

/**
 * Convert fanotify mask to event name string.
 *
 * @param mask  fanotify event mask
 * @return Event name string (statically allocated)
 */
const char *janus_mask_to_event_name(uint64_t mask);

/**
 * Get the number of active guards.
 *
 * @return Number of active guards
 */
int get_guard_count(void);

/**
 * Get the path from a fanotify event file descriptor.
 *
 * @param fd       File descriptor from fanotify event
 * @param path     Buffer to store path
 * @param pathlen  Size of path buffer
 * @return 0 on success, -1 on error
 */
int get_path_from_fd(int fd, char *path, size_t pathlen);

/**
 * Check if a path matches any pattern in a list.
 *
 * @param path      Path to check
 * @param patterns  Array of glob patterns
 * @param patternc  Number of patterns
 * @return true if matches, false otherwise
 */
bool path_matches_patterns(const char *path, const char *patterns[], unsigned int patternc);

#ifdef __cplusplus
}
#endif

#endif /* JANUSNOTIFY_H */
