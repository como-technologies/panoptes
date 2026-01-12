/**
 * Copyright 2026 Como Technologies, LTD
 * Licensed under the Apache License, Version 2.0
 *
 * Container runtime detection and PID lookup for containerd and CRI-O.
 * This module provides functions to detect the container runtime in use
 * and retrieve the init PID for containers.
 */

#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <errno.h>
#include <dirent.h>

#include "container_runtime.h"

/* Socket paths for container runtimes (relative to host root) */
#define CONTAINERD_SOCK_SUFFIX "/run/containerd/containerd.sock"
#define CRIO_SOCK_SUFFIX "/var/run/crio/crio.sock"

/* PID file path templates (relative to host root) */
#define CONTAINERD_PID_PATH_SUFFIX "/run/containerd/io.containerd.runtime.v2.task/k8s.io/%s/init.pid"
#define CRIO_PID_PATH_SUFFIX "/var/run/crio/%s/pidfile"

/**
 * Get host root prefix, auto-detecting or using env override.
 *
 * When running inside a container with host filesystem mounted at /host,
 * this returns "/host". Otherwise returns empty string.
 *
 * Behavior:
 * - HOST_ROOT_PATH not set or "auto" → auto-detect (/host if exists, else "")
 * - HOST_ROOT_PATH=/host → explicit /host prefix
 * - HOST_ROOT_PATH= (empty) → no prefix, use absolute paths directly
 */
static const char *get_host_root(void) {
    static const char *host_root = NULL;
    static int initialized = 0;

    if (!initialized) {
        initialized = 1;
        const char *env_val = getenv("HOST_ROOT_PATH");

        if (env_val != NULL && strcmp(env_val, "auto") != 0) {
            /* Explicit override - use as-is (including empty string) */
            host_root = env_val;
        } else {
            /* Auto-detect: check if /host/run exists */
            struct stat st;
            if (stat("/host/run", &st) == 0 && S_ISDIR(st.st_mode)) {
                host_root = "/host";
            } else {
                host_root = "";
            }
        }
    }
    return host_root;
}

/* Container ID prefixes */
#define CONTAINERD_PREFIX "containerd://"
#define CONTAINERD_PREFIX_LEN 13
#define CRIO_PREFIX "cri-o://"
#define CRIO_PREFIX_LEN 8

/**
 * Check if a file exists.
 */
static int file_exists(const char *path) {
    struct stat st;
    return stat(path, &st) == 0;
}

/**
 * Detect the container runtime based on available sockets.
 */
runtime_type_t detect_runtime(void) {
    char path[512];
    const char *root = get_host_root();

    snprintf(path, sizeof(path), "%s%s", root, CONTAINERD_SOCK_SUFFIX);
    if (file_exists(path)) {
        return RUNTIME_CONTAINERD;
    }

    snprintf(path, sizeof(path), "%s%s", root, CRIO_SOCK_SUFFIX);
    if (file_exists(path)) {
        return RUNTIME_CRIO;
    }

    return RUNTIME_UNKNOWN;
}

/**
 * Detect the container runtime from a container ID string.
 * Container IDs are prefixed with the runtime name (e.g., "containerd://abc123").
 */
runtime_type_t detect_runtime_from_id(const char *container_id) {
    if (container_id == NULL) {
        return RUNTIME_UNKNOWN;
    }

    if (strncmp(container_id, CONTAINERD_PREFIX, CONTAINERD_PREFIX_LEN) == 0) {
        return RUNTIME_CONTAINERD;
    }
    if (strncmp(container_id, CRIO_PREFIX, CRIO_PREFIX_LEN) == 0) {
        return RUNTIME_CRIO;
    }

    return RUNTIME_UNKNOWN;
}

/**
 * Get the container runtime name as a string.
 */
const char *runtime_name(runtime_type_t runtime) {
    switch (runtime) {
        case RUNTIME_CONTAINERD:
            return "containerd";
        case RUNTIME_CRIO:
            return "cri-o";
        default:
            return "unknown";
    }
}

/**
 * Strip the runtime prefix from a container ID.
 * Returns a pointer to the ID portion within the original string.
 */
const char *strip_container_id_prefix(const char *container_id) {
    if (container_id == NULL) {
        return NULL;
    }

    if (strncmp(container_id, CONTAINERD_PREFIX, CONTAINERD_PREFIX_LEN) == 0) {
        return container_id + CONTAINERD_PREFIX_LEN;
    }
    if (strncmp(container_id, CRIO_PREFIX, CRIO_PREFIX_LEN) == 0) {
        return container_id + CRIO_PREFIX_LEN;
    }

    /* No prefix, return as-is */
    return container_id;
}

/**
 * Read PID from a file.
 * Returns the PID on success, -1 on failure.
 */
static pid_t read_pid_file(const char *path) {
    FILE *f = fopen(path, "r");
    if (f == NULL) {
        return -1;
    }

    pid_t pid = -1;
    if (fscanf(f, "%d", &pid) != 1) {
        pid = -1;
    }

    fclose(f);
    return pid;
}

/**
 * Get the init PID for a container using containerd.
 */
static pid_t get_containerd_pid(const char *container_id) {
    char path[512];
    const char *id = strip_container_id_prefix(container_id);
    const char *root = get_host_root();

    snprintf(path, sizeof(path), "%s" CONTAINERD_PID_PATH_SUFFIX, root, id);
    return read_pid_file(path);
}

/**
 * Get the init PID for a container using CRI-O.
 */
static pid_t get_crio_pid(const char *container_id) {
    char path[512];
    const char *id = strip_container_id_prefix(container_id);
    const char *root = get_host_root();

    snprintf(path, sizeof(path), "%s" CRIO_PID_PATH_SUFFIX, root, id);
    return read_pid_file(path);
}

/**
 * Get the init PID for a container.
 * Automatically detects the runtime from the container ID prefix.
 */
pid_t get_container_pid(const char *container_id) {
    runtime_type_t runtime = detect_runtime_from_id(container_id);

    switch (runtime) {
        case RUNTIME_CONTAINERD:
            return get_containerd_pid(container_id);
        case RUNTIME_CRIO:
            return get_crio_pid(container_id);
        default:
            /* Try both if runtime not detected from ID */
            runtime = detect_runtime();
            if (runtime == RUNTIME_CONTAINERD) {
                return get_containerd_pid(container_id);
            } else if (runtime == RUNTIME_CRIO) {
                return get_crio_pid(container_id);
            }
            return -1;
    }
}

/**
 * Get the init PID for a container with explicit runtime type.
 */
pid_t get_container_pid_with_runtime(const char *container_id, runtime_type_t runtime) {
    switch (runtime) {
        case RUNTIME_CONTAINERD:
            return get_containerd_pid(container_id);
        case RUNTIME_CRIO:
            return get_crio_pid(container_id);
        case RUNTIME_AUTO:
            return get_container_pid(container_id);
        default:
            return -1;
    }
}

/**
 * Get the root filesystem path for a container's process.
 * Returns 0 on success, -1 on failure.
 * The path buffer must be at least PATH_MAX bytes.
 */
int get_container_rootfs(pid_t pid, char *path, size_t path_size) {
    char proc_root[64];
    snprintf(proc_root, sizeof(proc_root), "/proc/%d/root", pid);

    ssize_t len = readlink(proc_root, path, path_size - 1);
    if (len == -1) {
        return -1;
    }

    path[len] = '\0';
    return 0;
}

/**
 * Resolve a path within a container's filesystem.
 * Combines the container's root filesystem path with the given path.
 * Returns 0 on success, -1 on failure.
 */
int resolve_container_path(pid_t pid, const char *container_path, char *resolved, size_t resolved_size) {
    char rootfs[PATH_MAX];

    if (get_container_rootfs(pid, rootfs, sizeof(rootfs)) != 0) {
        return -1;
    }

    /* Remove trailing slash from rootfs if present */
    size_t rootfs_len = strlen(rootfs);
    if (rootfs_len > 0 && rootfs[rootfs_len - 1] == '/') {
        rootfs[rootfs_len - 1] = '\0';
    }

    /* Combine paths */
    int ret = snprintf(resolved, resolved_size, "%s%s", rootfs, container_path);
    if (ret < 0 || (size_t)ret >= resolved_size) {
        return -1;
    }

    return 0;
}

/**
 * Check if a process is still running.
 */
int is_process_running(pid_t pid) {
    char proc_path[64];
    snprintf(proc_path, sizeof(proc_path), "/proc/%d", pid);
    return file_exists(proc_path);
}

/**
 * Get child PIDs of a process (for tracking container subprocesses).
 * Returns the number of child PIDs found, or -1 on error.
 * The pids array must be pre-allocated with max_pids entries.
 */
int get_child_pids(pid_t parent_pid, pid_t *pids, int max_pids) {
    char children_path[128];
    snprintf(children_path, sizeof(children_path), "/proc/%d/task/%d/children", parent_pid, parent_pid);

    FILE *f = fopen(children_path, "r");
    if (f == NULL) {
        return -1;
    }

    int count = 0;
    pid_t child_pid;
    while (count < max_pids && fscanf(f, "%d", &child_pid) == 1) {
        pids[count++] = child_pid;
    }

    fclose(f);
    return count;
}
