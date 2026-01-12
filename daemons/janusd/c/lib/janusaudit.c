/**
 * Copyright 2026 Como Technologies, LTD
 * Licensed under the Apache License, Version 2.0
 *
 * Janus kernel audit log interface.
 * Writes access events to the Linux audit subsystem.
 */

#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <time.h>
#include <sys/socket.h>
#include <linux/audit.h>
#include <linux/netlink.h>

#include "janusaudit.h"

/* Netlink socket for audit communication */
static int audit_fd = -1;
static uint32_t audit_seq = 0;

/* Custom audit message type for Janus (in user range) */
#define JANUS_AUDIT_TYPE 1400

int janus_audit_init(void) {
    if (audit_fd >= 0) {
        return 0;  /* Already initialized */
    }

    audit_fd = socket(PF_NETLINK, SOCK_RAW | SOCK_CLOEXEC, NETLINK_AUDIT);
    if (audit_fd < 0) {
        return -1;
    }

    struct sockaddr_nl addr;
    memset(&addr, 0, sizeof(addr));
    addr.nl_family = AF_NETLINK;
    addr.nl_pid = getpid();
    addr.nl_groups = 0;

    if (bind(audit_fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        close(audit_fd);
        audit_fd = -1;
        return -1;
    }

    return 0;
}

void janus_audit_close(void) {
    if (audit_fd >= 0) {
        close(audit_fd);
        audit_fd = -1;
    }
}

bool janus_audit_available(void) {
    return audit_fd >= 0;
}

int janus_audit_write(const janus_audit_event_t *event) {
    if (audit_fd < 0 || event == NULL) {
        return -1;
    }

    /* Format the audit message */
    char msg[1024];
    int len = janus_audit_format(event, msg, sizeof(msg));
    if (len < 0) {
        return -1;
    }

    /* Build netlink message */
    struct {
        struct nlmsghdr nlh;
        char data[1024];
    } req;

    memset(&req, 0, sizeof(req));
    req.nlh.nlmsg_len = NLMSG_SPACE(len);
    req.nlh.nlmsg_type = AUDIT_USER;
    req.nlh.nlmsg_flags = NLM_F_REQUEST;
    req.nlh.nlmsg_seq = ++audit_seq;
    req.nlh.nlmsg_pid = getpid();

    memcpy(NLMSG_DATA(&req.nlh), msg, len);

    struct sockaddr_nl dst;
    memset(&dst, 0, sizeof(dst));
    dst.nl_family = AF_NETLINK;
    dst.nl_pid = 0;  /* Send to kernel */
    dst.nl_groups = 0;

    if (sendto(audit_fd, &req, req.nlh.nlmsg_len, 0,
               (struct sockaddr *)&dst, sizeof(dst)) < 0) {
        return -1;
    }

    return 0;
}

int janus_audit_log_access(
    const char *guard_name,
    const char *pod_name,
    const char *path,
    pid_t pid,
    uid_t uid,
    bool allowed
) {
    janus_audit_event_t event;
    memset(&event, 0, sizeof(event));

    event.type = allowed ? JANUS_AUDIT_ACCESS : JANUS_AUDIT_DENIED;
    event.guard_name = guard_name;
    event.pod_name = pod_name;
    event.path = path;
    event.pid = pid;
    event.uid = uid;
    event.allowed = allowed;
    event.timestamp = time(NULL);

    return janus_audit_write(&event);
}

int janus_audit_format(const janus_audit_event_t *event, char *buffer, size_t buflen) {
    if (event == NULL || buffer == NULL || buflen == 0) {
        return -1;
    }

    const char *type_str;
    switch (event->type) {
        case JANUS_AUDIT_ACCESS:
            type_str = "ACCESS";
            break;
        case JANUS_AUDIT_OPEN:
            type_str = "OPEN";
            break;
        case JANUS_AUDIT_DENIED:
            type_str = "DENIED";
            break;
        case JANUS_AUDIT_POLICY:
            type_str = "POLICY";
            break;
        default:
            type_str = "UNKNOWN";
    }

    int len = snprintf(buffer, buflen,
        "op=janus type=%s guard=\"%s\" pod=\"%s\" path=\"%s\" "
        "pid=%d uid=%d allowed=%s",
        type_str,
        event->guard_name ? event->guard_name : "",
        event->pod_name ? event->pod_name : "",
        event->path ? event->path : "",
        event->pid,
        event->uid,
        event->allowed ? "yes" : "no"
    );

    if (len < 0 || (size_t)len >= buflen) {
        return -1;
    }

    /* Add reason if present */
    if (event->reason != NULL && event->reason[0] != '\0') {
        int extra = snprintf(buffer + len, buflen - len, " reason=\"%s\"", event->reason);
        if (extra > 0 && (size_t)(len + extra) < buflen) {
            len += extra;
        }
    }

    return len;
}
