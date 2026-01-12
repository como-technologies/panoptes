# Panoptes Common Library
# Shared utilities for argusd and janusd daemons.

cmake_minimum_required(VERSION 3.16)

# Pure C library for container runtime detection
add_library(panoptes_common STATIC
    container_runtime.c
)

target_include_directories(panoptes_common PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}
)

# C99 standard
set_target_properties(panoptes_common PROPERTIES
    C_STANDARD 99
    C_STANDARD_REQUIRED ON
)
