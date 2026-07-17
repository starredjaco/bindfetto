cmake_minimum_required(VERSION 3.16)
project(bindfetto_dlt_plugin LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

# A DLT Viewer decoder plugin needs only QtCore (QString/QStringList/QObject).
find_package(QT NAMES Qt6 Qt5 REQUIRED COMPONENTS Core)
find_package(Qt${QT_VERSION_MAJOR} REQUIRED COMPONENTS Core)

# --- dlt-viewer qdlt SDK (headers only) ------------------------------------------
# Only the qdlt *headers* are needed: DLT Viewer dlopen's the plugin and already has
# qdlt's symbols in its process, so libqdlt is never linked — the symbols resolve from
# the host at load time (matches the stock plugins, keeps the .so portable). A *fresh*
# dlt-viewer checkout — no build/ — is enough.
# Easiest: point DLT_VIEWER_DIR at a dlt-viewer checkout:
#   -DDLT_VIEWER_DIR=/path/to/dlt-viewer   (expects <DLT_VIEWER_DIR>/qdlt/*.h)
# Advanced (non-standard layout): set DLT_VIEWER_QDLT_INCLUDE_DIR directly.
set(DLT_VIEWER_DIR "" CACHE PATH
    "dlt-viewer checkout root; derives qdlt headers")
set(DLT_VIEWER_QDLT_INCLUDE_DIR "" CACHE PATH
    "dlt-viewer qdlt headers (default: <DLT_VIEWER_DIR>/qdlt)")

if(DLT_VIEWER_DIR AND NOT DLT_VIEWER_QDLT_INCLUDE_DIR)
    set(DLT_VIEWER_QDLT_INCLUDE_DIR "${DLT_VIEWER_DIR}/qdlt")
endif()

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

# qdlt symbols are never linked — they resolve from the DLT Viewer process at dlopen,
# so the plugin stays portable (matches the stock plugins).
if(APPLE)
    # Mach-O needs an explicit flag to leave the symbols undefined.
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
