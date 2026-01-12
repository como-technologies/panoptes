# Dependencies.cmake
# Shared FetchContent declarations for gflags, glog, and fmt
# Used by both argusd and janusd for static builds

include(FetchContent)

# Prevent re-fetching if already done
if(NOT TARGET gflags::gflags)
    # gflags - command line flags parsing
    FetchContent_Declare(gflags
        GIT_REPOSITORY https://github.com/gflags/gflags.git
        GIT_TAG        v2.2.2
        GIT_SHALLOW    TRUE
    )
    set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
    set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
    set(GFLAGS_BUILD_STATIC_LIBS ON CACHE BOOL "" FORCE)
    set(GFLAGS_BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
    set(GFLAGS_BUILD_gflags_LIB ON CACHE BOOL "" FORCE)
    set(GFLAGS_BUILD_gflags_nothreads_LIB OFF CACHE BOOL "" FORCE)
endif()

if(NOT TARGET glog::glog)
    # glog - Google logging library
    FetchContent_Declare(glog
        GIT_REPOSITORY https://github.com/google/glog.git
        GIT_TAG        v0.7.1
        GIT_SHALLOW    TRUE
    )
    set(WITH_GFLAGS ON CACHE BOOL "" FORCE)
    set(WITH_GTEST OFF CACHE BOOL "" FORCE)
    set(WITH_UNWIND OFF CACHE BOOL "" FORCE)
    set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
    set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
endif()

if(NOT TARGET fmt::fmt)
    # fmt - modern formatting library
    FetchContent_Declare(fmt
        GIT_REPOSITORY https://github.com/fmtlib/fmt.git
        GIT_TAG        10.2.1
        GIT_SHALLOW    TRUE
    )
    set(FMT_TEST OFF CACHE BOOL "" FORCE)
    set(FMT_DOC OFF CACHE BOOL "" FORCE)
    set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
endif()

# Fetch all dependencies - order matters (gflags before glog)
FetchContent_MakeAvailable(gflags glog fmt)
