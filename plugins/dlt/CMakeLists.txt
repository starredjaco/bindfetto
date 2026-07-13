cmake_minimum_required(VERSION 3.16)
project(bindfetto_dlt_plugin LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

# A DLT Viewer decoder plugin needs only QtCore (QString/QStringList/QObject).
find_package(QT NAMES Qt6 Qt5 REQUIRED COMPONENTS Core)
find_package(Qt${QT_VERSION_MAJOR} REQUIRED COMPONENTS Core)

# --- dlt-viewer qdlt SDK (headers + library) -------------------------------------
# Easiest: point DLT_VIEWER_DIR at your (built) dlt-viewer checkout — the qdlt headers
# and libqdlt are derived from it:
#   -DDLT_VIEWER_DIR=/path/to/dlt-viewer
# expecting <DLT_VIEWER_DIR>/qdlt/*.h and <DLT_VIEWER_DIR>/build/bin/libqdlt.
# Advanced (non-standard layout): set the two specific vars below instead.
set(DLT_VIEWER_DIR "" CACHE PATH
    "dlt-viewer checkout root; derives qdlt headers + built libqdlt")
set(DLT_VIEWER_QDLT_INCLUDE_DIR "" CACHE PATH
    "dlt-viewer qdlt headers (default: <DLT_VIEWER_DIR>/qdlt)")

if(DLT_VIEWER_DIR AND NOT DLT_VIEWER_QDLT_INCLUDE_DIR)
    set(DLT_VIEWER_QDLT_INCLUDE_DIR "${DLT_VIEWER_DIR}/qdlt")
endif()

# libqdlt lands in the checkout's build output; also honor default search paths.
find_library(DLT_VIEWER_QDLT_LIB NAMES qdlt
    HINTS "${DLT_VIEWER_DIR}/build/bin"
    DOC "dlt-viewer qdlt shared library (libqdlt), under <DLT_VIEWER_DIR>/build/bin")

# --- bindfetto Rust decode core (C ABI) ------------------------------------------
set(BINDFETTO_DECODE_INCLUDE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../decode/include"
    CACHE PATH "location of bindfetto_decode.h")
# Prefer the *static* archive so the core is embedded in the plugin (a self-contained
# .so with no dev-path dylib dependency). Restrict the suffix search to .a.
set(_bf_saved_suffixes ${CMAKE_FIND_LIBRARY_SUFFIXES})
set(CMAKE_FIND_LIBRARY_SUFFIXES .a)
find_library(BINDFETTO_DECODE_LIB NAMES bindfetto_decode
    PATHS "${CMAKE_CURRENT_SOURCE_DIR}/../../decode/target/release"
    NO_DEFAULT_PATH
    DOC "Rust decode core static library (build with: cargo build --release in decode/)")
set(CMAKE_FIND_LIBRARY_SUFFIXES ${_bf_saved_suffixes})

# Only the qdlt *headers* are mandatory: the plugin is dlopen'd by DLT Viewer, which
# already has qdlt's symbols in its process, so libqdlt can be left to resolve at load
# time (see the link section below). That means a *fresh* dlt-viewer checkout — no
# build/ — is enough; building dlt-viewer is only needed if you want the plugin to link
# libqdlt directly.
if(NOT DLT_VIEWER_QDLT_INCLUDE_DIR)
    message(FATAL_ERROR
        "dlt-viewer qdlt headers not found. Pass -DDLT_VIEWER_DIR=/path/to/dlt-viewer "
        "(a checkout — its qdlt/ headers suffice), or set DLT_VIEWER_QDLT_INCLUDE_DIR "
        "directly. See README.md")
endif()
if(NOT EXISTS "${DLT_VIEWER_QDLT_INCLUDE_DIR}/plugininterface.h")
    message(FATAL_ERROR
        "DLT_VIEWER_QDLT_INCLUDE_DIR=${DLT_VIEWER_QDLT_INCLUDE_DIR} has no plugininterface.h "
        "— check DLT_VIEWER_DIR points at a dlt-viewer checkout (expects qdlt/ there).")
endif()
if(NOT DLT_VIEWER_QDLT_LIB)
    message(STATUS
        "libqdlt not found (no dlt-viewer build) — building against qdlt headers only; "
        "its symbols resolve from the DLT Viewer process when the plugin loads.")
endif()
if(NOT BINDFETTO_DECODE_LIB)
    message(FATAL_ERROR
        "libbindfetto_decode not found. Build it: (cd decode && cargo build --release)")
endif()

add_library(bindfettodecoderplugin MODULE
    bindfettodecoderplugin.cpp
    bindfettodecoderplugin.h
)

target_include_directories(bindfettodecoderplugin PRIVATE
    ${DLT_VIEWER_QDLT_INCLUDE_DIR}
    ${BINDFETTO_DECODE_INCLUDE_DIR}
)

target_link_libraries(bindfettodecoderplugin PRIVATE
    Qt${QT_VERSION_MAJOR}::Core
    ${BINDFETTO_DECODE_LIB}
)

if(DLT_VIEWER_QDLT_LIB)
    # Link libqdlt directly when a dlt-viewer build is available.
    target_link_libraries(bindfettodecoderplugin PRIVATE ${DLT_VIEWER_QDLT_LIB})
elseif(APPLE)
    # Headers-only: leave qdlt symbols undefined, resolved from the host at dlopen.
    target_link_options(bindfettodecoderplugin PRIVATE -undefined dynamic_lookup)
endif()
# On ELF (Linux) a MODULE already permits undefined symbols, so no flag is needed there.

# The Rust static library pulls in platform runtime deps at final link.
if(APPLE)
    target_link_libraries(bindfettodecoderplugin PRIVATE
        "-framework CoreFoundation" "-framework Security")
elseif(UNIX)
    target_link_libraries(bindfettodecoderplugin PRIVATE pthread dl m)
endif()
