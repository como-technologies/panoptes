/**
 * Copyright 2026 Como Technologies, LTD
 * Licensed under the Apache License, Version 2.0
 *
 * Janus kernel audit log interface header.
 */

#ifndef __JANUS_AUDIT_H__
#define __JANUS_AUDIT_H__

#include <stdbool.h>
#include <stdint.h>
#include <sys/types.h>
#include <time.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Janus audit event types.
 */
typedef enum {
    JANUS_AUDIT_ACCESS = 0,
    JANUS_AUDIT_OPEN,
    JANUS_AUDIT_DENIED,
    JANUS_AUDIT_POLICY
} janus_audit_type_t;

/**
 * Janus audit event structure.
 */
typedef struct {
    janus_audit_type_t type;
    const char *guard_name;
    const char *pod_name;
    const char *path;
    pid_t pid;
    uid_t uid;
    bool allowed;
    time_t timestamp;
    const char *reason;
} janus_audit_event_t;

/**
 * Initialize the audit subsystem.
 * Returns 0 on success, -1 on failure.
 */
int janus_audit_init(void);

/**
 * Close the audit subsystem.
 */
void janus_audit_close(void);

/**
 * Check if audit is available.
 */
bool janus_audit_available(void);

/**
 * Write an audit event.
 * Returns 0 on success, -1 on failure.
 */
int janus_audit_write(const janus_audit_event_t *event);

/**
 * Convenience function to log an access event.
 * Returns 0 on success, -1 on failure.
 */
int janus_audit_log_access(
    const char *guard_name,
    const char *pod_name,
    const char *path,
    pid_t pid,
    uid_t uid,
    bool allowed
);

/**
 * Format an audit event as a string.
 * Returns the length of the formatted string, or -1 on error.
 */
int janus_audit_format(const janus_audit_event_t *event, char *buffer, size_t buflen);

#ifdef __cplusplus
}
#endif

#endif /* __JANUS_AUDIT_H__ */
