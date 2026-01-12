/**
 * Copyright 2026 Como Technologies, LTD
 * Licensed under the Apache License, Version 2.0
 *
 * Argus directory tree management for recursive watching.
 * Manages hierarchical watch descriptors and path mappings.
 */

#ifndef ARGUSTREE_H
#define ARGUSTREE_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <stdbool.h>

/**
 * Opaque handle for a watch tree.
 */
typedef struct argus_tree argus_tree_t;

/**
 * Tree node representing a watched path.
 */
typedef struct {
    int wd;                 /* inotify watch descriptor */
    char *path;             /* Full path */
    int depth;              /* Depth from root */
    bool is_dir;            /* Whether this is a directory */
} argus_tree_node_t;

/**
 * Callback for tree traversal.
 */
typedef void (*argus_tree_visit_fn)(argus_tree_node_t *node, void *userdata);

/**
 * Create a new watch tree.
 *
 * @param root_path  The root path of the tree
 * @return Tree handle, or NULL on error
 */
argus_tree_t *argus_tree_create(const char *root_path);

/**
 * Destroy a watch tree and free all resources.
 *
 * @param tree  Tree handle to destroy
 */
void argus_tree_destroy(argus_tree_t *tree);

/**
 * Add a path to the tree.
 *
 * @param tree   Tree handle
 * @param path   Path to add
 * @param wd     inotify watch descriptor
 * @param is_dir Whether the path is a directory
 * @return 0 on success, -1 on error
 */
int argus_tree_add(argus_tree_t *tree, const char *path, int wd, bool is_dir);

/**
 * Remove a path from the tree.
 *
 * @param tree  Tree handle
 * @param path  Path to remove
 * @return 0 on success, -1 if not found
 */
int argus_tree_remove(argus_tree_t *tree, const char *path);

/**
 * Remove a path by watch descriptor.
 *
 * @param tree  Tree handle
 * @param wd    Watch descriptor to remove
 * @return 0 on success, -1 if not found
 */
int argus_tree_remove_by_wd(argus_tree_t *tree, int wd);

/**
 * Find a node by path.
 *
 * @param tree  Tree handle
 * @param path  Path to find
 * @return Node pointer, or NULL if not found
 */
argus_tree_node_t *argus_tree_find(argus_tree_t *tree, const char *path);

/**
 * Find a node by watch descriptor.
 *
 * @param tree  Tree handle
 * @param wd    Watch descriptor to find
 * @return Node pointer, or NULL if not found
 */
argus_tree_node_t *argus_tree_find_by_wd(argus_tree_t *tree, int wd);

/**
 * Get the path for a watch descriptor.
 *
 * @param tree  Tree handle
 * @param wd    Watch descriptor
 * @return Path string (owned by tree), or NULL if not found
 */
const char *argus_tree_get_path(argus_tree_t *tree, int wd);

/**
 * Get the number of nodes in the tree.
 *
 * @param tree  Tree handle
 * @return Number of nodes
 */
int argus_tree_count(argus_tree_t *tree);

/**
 * Get the maximum depth of the tree.
 *
 * @param tree  Tree handle
 * @return Maximum depth
 */
int argus_tree_max_depth(argus_tree_t *tree);

/**
 * Traverse the tree and call the visitor function for each node.
 *
 * @param tree      Tree handle
 * @param visitor   Callback function
 * @param userdata  User data passed to callback
 */
void argus_tree_traverse(argus_tree_t *tree, argus_tree_visit_fn visitor, void *userdata);

/**
 * Build a tree by recursively scanning a directory.
 *
 * @param tree       Tree handle
 * @param inotify_fd inotify file descriptor
 * @param mask       inotify event mask for new watches
 * @param maxdepth   Maximum recursion depth (0 = unlimited)
 * @param ignores    Array of glob patterns to ignore
 * @param ignorec    Number of ignore patterns
 * @return Number of watches added, or -1 on error
 */
int argus_tree_build_recursive(
    argus_tree_t *tree,
    int inotify_fd,
    uint32_t mask,
    int maxdepth,
    const char *ignores[],
    unsigned int ignorec
);

#ifdef __cplusplus
}
#endif

#endif /* ARGUSTREE_H */
