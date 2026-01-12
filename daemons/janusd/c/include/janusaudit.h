/**
 * Copyright 2026 Como Technologies, LTD
 * Licensed under the Apache License, Version 2.0
 *
 * Janus kernel audit log interface.
 * Writes access events to the Linux audit subsystem.
 */

#ifndef JANUSAUDIT_H
#define JANUSAUDIT_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <stdbool.h>
#include <sys/types.h>

/**
 * Audit event types.
 */
typedef enum {
    JANUS_AUDIT_ACCESS = 1,       /* File access */
    JANUS_AUDIT_OPEN = 2,         /* File open */
    JANUS_AUDIT_DENIED = 3,       /* Access denied */
    JANUS_AUDIT_POLICY = 4        /* Policy change */
} janus_audit_type_t;

/**
 * Audit event structure.
 */
typedef struct {
    janus_audit_type_t type;
    const char *guard_name;
    const char *pod_name;
    const char *path;
    pid_t pid;
    uid_t uid;
    gid_t gid;
    bool allowed;
    const char *reason;           /* Reason for deny if applicable */
    int64_t timestamp;
} janus_audit_event_t;

/**
 * Initialize the audit subsystem.
 *
 * @return 0 on success, -1 on error
 */
int janus_audit_init(void);

/**
 * Close the audit subsystem.
 */
void janus_audit_close(void);

/**
 * Check if audit is available and initialized.
 *
 * @return true if available, false otherwise
 */
bool janus_audit_available(void);

/**
 * Write an audit event to the kernel audit log.
 *
 * @param event  Audit event to write
 * @return 0 on success, -1 on error
 */
int janus_audit_write(const janus_audit_event_t *event);

/**
 * Write a simple access audit message.
 *
 * @param guard_name  Guard name
 * @param pod_name    Pod name
 * @param path        File path
 * @param pid         Process ID
 * @param uid         User ID
 * @param allowed     Whether access was allowed
 * @return 0 on success, -1 on error
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
 * Format an audit message for logging.
 *
 * @param event   Audit event
 * @param buffer  Output buffer
 * @param buflen  Buffer size
 * @return Number of bytes written, or -1 on error
 */
int janus_audit_format(const janus_audit_event_t *event, char *buffer, size_t buflen);

#ifdef __cplusplus
}
#endif

#endif /* JANUSAUDIT_H */
