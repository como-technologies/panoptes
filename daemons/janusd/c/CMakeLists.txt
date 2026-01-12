cmake_minimum_required(VERSION 3.16)

project(janusd
    VERSION 2.0.0
    DESCRIPTION "Janus Daemon - File Access Auditing for Kubernetes"
    HOMEPAGE_URL "https://github.com/como-technologies/panoptes"
    LANGUAGES C CXX
)

set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

# Options
option(BUILD_TESTING "Build tests" OFF)
option(BUILD_STATIC "Build fully static binary" ON)

# Proto directory - set by Dockerfile or override for local builds
if(NOT DEFINED PROTO_ROOT)
    set(PROTO_ROOT "/proto")
endif()

# Common library directory
if(NOT DEFINED COMMON_LIB_DIR)
    set(COMMON_LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../common/lib")
endif()

# Compiler warnings
add_compile_options(-Wall -Wextra -Wno-unused-parameter)

# Find packages
find_package(PkgConfig REQUIRED)
find_package(Threads REQUIRED)

# Static build setup - use CMake find_package for installed static libs
if(BUILD_STATIC)
    # Prefer static libraries
    set(CMAKE_FIND_LIBRARY_SUFFIXES ".a")
    set(BUILD_SHARED_LIBS OFF)

    # Static linking flags
    set(CMAKE_EXE_LINKER_FLAGS "${CMAKE_EXE_LINKER_FLAGS} -static")

    # Find gRPC and protobuf via CMake config (installed by grpc-static-builder)
    find_package(gRPC CONFIG REQUIRED)
    find_package(protobuf CONFIG REQUIRED)
    find_package(absl CONFIG REQUIRED)

    # Get the grpc_cpp_plugin location
    get_target_property(GRPC_CPP_PLUGIN gRPC::grpc_cpp_plugin LOCATION)
    get_target_property(PROTOC protobuf::protoc LOCATION)

    # Fetch gflags, glog, fmt via FetchContent (provides proper CMake targets)
    include(${COMMON_LIB_DIR}/../cmake/Dependencies.cmake)

    # Static libraries list for final linking
    set(STATIC_LIBS
        gRPC::grpc++_reflection
        gRPC::grpc++
        gRPC::grpc
        protobuf::libprotobuf
        absl::base
        absl::flags
        absl::log
        absl::strings
        absl::synchronization
        absl::time
        absl::status
        absl::statusor
        glog::glog
        gflags::gflags
        fmt::fmt
        ssl
        crypto
        z
        re2
        Threads::Threads
        dl
    )
else()
    # Shared library build (development/quick iteration)
    pkg_check_modules(GRPC REQUIRED grpc++ grpc)
    pkg_check_modules(PROTOBUF REQUIRED protobuf)
    include_directories(${GRPC_INCLUDE_DIRS} ${PROTOBUF_INCLUDE_DIRS})
    link_directories(${GRPC_LIBRARY_DIRS} ${PROTOBUF_LIBRARY_DIRS})

    pkg_check_modules(GLOG REQUIRED libglog)
    pkg_check_modules(GFLAGS REQUIRED gflags)
    pkg_check_modules(FMT REQUIRED fmt)
    include_directories(${GLOG_INCLUDE_DIRS} ${GFLAGS_INCLUDE_DIRS} ${FMT_INCLUDE_DIRS})
    link_directories(${GLOG_LIBRARY_DIRS} ${GFLAGS_LIBRARY_DIRS} ${FMT_LIBRARY_DIRS})

    find_program(PROTOC protoc REQUIRED)
    find_program(GRPC_CPP_PLUGIN grpc_cpp_plugin REQUIRED)

    set(SHARED_LIBS
        ${GRPC_LIBRARIES}
        ${PROTOBUF_LIBRARIES}
        ${GLOG_LIBRARIES}
        ${GFLAGS_LIBRARIES}
        ${FMT_LIBRARIES}
        grpc++_reflection
        Threads::Threads
    )
endif()

# Add common library
add_subdirectory(${COMMON_LIB_DIR} ${CMAKE_CURRENT_BINARY_DIR}/common)

# Core fanotify library in pure C
add_library(janusnotify STATIC
    lib/janusnotify.c
    lib/janusutil.c
    lib/janusaudit.c
)
target_include_directories(janusnotify PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/lib
    ${COMMON_LIB_DIR}
)
target_link_libraries(janusnotify PUBLIC panoptes_common)

# Proto generation from top-level proto/ directory
set(PROTO_OUT_DIR ${CMAKE_CURRENT_BINARY_DIR}/gen)
file(MAKE_DIRECTORY ${PROTO_OUT_DIR})
file(MAKE_DIRECTORY ${PROTO_OUT_DIR}/janus/v1)
file(MAKE_DIRECTORY ${PROTO_OUT_DIR}/grpc/health/v1)

# Janus v1 proto files - generated with proper directory structure
set(JANUS_PROTO_SRCS
    ${PROTO_OUT_DIR}/janus/v1/janus.pb.cc
)
set(JANUS_GRPC_SRCS
    ${PROTO_OUT_DIR}/janus/v1/janus.grpc.pb.cc
)
set(JANUS_PROTO_HDRS
    ${PROTO_OUT_DIR}/janus/v1/janus.pb.h
    ${PROTO_OUT_DIR}/janus/v1/janus.grpc.pb.h
)

# Health proto files (standard gRPC health checking)
set(HEALTH_PROTO_SRCS
    ${PROTO_OUT_DIR}/grpc/health/v1/health.pb.cc
)
set(HEALTH_GRPC_SRCS
    ${PROTO_OUT_DIR}/grpc/health/v1/health.grpc.pb.cc
)
set(HEALTH_PROTO_HDRS
    ${PROTO_OUT_DIR}/grpc/health/v1/health.pb.h
    ${PROTO_OUT_DIR}/grpc/health/v1/health.grpc.pb.h
)

# Generate janus.proto with proper package structure
add_custom_command(
    OUTPUT ${PROTO_OUT_DIR}/janus/v1/janus.pb.cc ${PROTO_OUT_DIR}/janus/v1/janus.pb.h
           ${PROTO_OUT_DIR}/janus/v1/janus.grpc.pb.cc ${PROTO_OUT_DIR}/janus/v1/janus.grpc.pb.h
    COMMAND ${PROTOC}
        --proto_path=${PROTO_ROOT}
        --cpp_out=${PROTO_OUT_DIR}
        --grpc_out=${PROTO_OUT_DIR}
        --plugin=protoc-gen-grpc=${GRPC_CPP_PLUGIN}
        ${PROTO_ROOT}/janus/v1/janus.proto
    DEPENDS ${PROTO_ROOT}/janus/v1/janus.proto
    COMMENT "Generating C++ sources from janus/v1/janus.proto"
)

# Generate health.proto (gRPC health checking)
add_custom_command(
    OUTPUT ${PROTO_OUT_DIR}/grpc/health/v1/health.pb.cc ${PROTO_OUT_DIR}/grpc/health/v1/health.pb.h
           ${PROTO_OUT_DIR}/grpc/health/v1/health.grpc.pb.cc ${PROTO_OUT_DIR}/grpc/health/v1/health.grpc.pb.h
    COMMAND ${PROTOC}
        --proto_path=${PROTO_ROOT}
        --cpp_out=${PROTO_OUT_DIR}
        --grpc_out=${PROTO_OUT_DIR}
        --plugin=protoc-gen-grpc=${GRPC_CPP_PLUGIN}
        ${PROTO_ROOT}/grpc/health/v1/health.proto
    DEPENDS ${PROTO_ROOT}/grpc/health/v1/health.proto
    COMMENT "Generating C++ sources from grpc/health/v1/health.proto"
)

# Custom target for proto generation
add_custom_target(janus_proto_gen DEPENDS
    ${JANUS_PROTO_SRCS} ${JANUS_GRPC_SRCS} ${JANUS_PROTO_HDRS}
    ${HEALTH_PROTO_SRCS} ${HEALTH_GRPC_SRCS} ${HEALTH_PROTO_HDRS}
)

# Mark generated files
set_source_files_properties(
    ${JANUS_PROTO_SRCS} ${JANUS_GRPC_SRCS}
    ${HEALTH_PROTO_SRCS} ${HEALTH_GRPC_SRCS}
    PROPERTIES GENERATED TRUE
)

# gRPC server executable
add_executable(janusd
    src/janusd_server.cc
    src/janusd_impl.cc
    src/health_impl.cc
    ${JANUS_PROTO_SRCS}
    ${JANUS_GRPC_SRCS}
    ${HEALTH_PROTO_SRCS}
    ${HEALTH_GRPC_SRCS}
)

# Ensure proto files are generated before compiling
add_dependencies(janusd janus_proto_gen)

target_include_directories(janusd PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_CURRENT_SOURCE_DIR}/lib
    ${CMAKE_CURRENT_SOURCE_DIR}/src
    ${COMMON_LIB_DIR}
    ${PROTO_OUT_DIR}
)

# Link libraries based on build type
if(BUILD_STATIC)
    target_link_libraries(janusd PRIVATE
        janusnotify
        panoptes_common
        ${STATIC_LIBS}
    )
else()
    target_link_libraries(janusd PRIVATE
        janusnotify
        panoptes_common
        ${SHARED_LIBS}
    )
endif()

# Strip binary in release mode
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    add_custom_command(TARGET janusd POST_BUILD
        COMMAND ${CMAKE_STRIP} -s $<TARGET_FILE:janusd>
        COMMENT "Stripping janusd binary"
    )
endif()

# Install
install(TARGETS janusd DESTINATION bin)
