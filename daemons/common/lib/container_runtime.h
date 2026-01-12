/**
 * Copyright 2026 Como Technologies, LTD
 * Licensed under the Apache License, Version 2.0
 *
 * Container runtime detection and PID lookup header.
 */

#ifndef __CONTAINER_RUNTIME_H__
#define __CONTAINER_RUNTIME_H__

#include <sys/types.h>
#include <limits.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Supported container runtime types.
 */
typedef enum {
    RUNTIME_UNKNOWN = 0,
    RUNTIME_CONTAINERD,
    RUNTIME_CRIO,
    RUNTIME_AUTO
} runtime_type_t;

/**
 * Detect the container runtime based on available sockets.
 */
runtime_type_t detect_runtime(void);

/**
 * Detect the container runtime from a container ID string.
 */
runtime_type_t detect_runtime_from_id(const char *container_id);

/**
 * Get the container runtime name as a string.
 */
const char *runtime_name(runtime_type_t runtime);

/**
 * Strip the runtime prefix from a container ID.
 */
const char *strip_container_id_prefix(const char *container_id);

/**
 * Get the init PID for a container.
 */
pid_t get_container_pid(const char *container_id);

/**
 * Get the init PID for a container with explicit runtime type.
 */
pid_t get_container_pid_with_runtime(const char *container_id, runtime_type_t runtime);

/**
 * Get the root filesystem path for a container's process.
 */
int get_container_rootfs(pid_t pid, char *path, size_t path_size);

/**
 * Resolve a path within a container's filesystem.
 */
int resolve_container_path(pid_t pid, const char *container_path, char *resolved, size_t resolved_size);

/**
 * Check if a process is still running.
 */
int is_process_running(pid_t pid);

/**
 * Get child PIDs of a process.
 */
int get_child_pids(pid_t parent_pid, pid_t *pids, int max_pids);

#ifdef __cplusplus
}
#endif

#endif /* __CONTAINER_RUNTIME_H__ */
