cmake_minimum_required(VERSION 3.16)
project(pskk-fcitx5 VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

include(GNUInstallDirs)

# ---------------------------------------------------------------------------
# Locate fcitx5 (headers + libraries).
#
# Distros package the fcitx5 core under different names:
#   Debian/Ubuntu : pkg-config modules Fcitx5Core / Fcitx5Utils / Fcitx5Config,
#                   headers under /usr/include/Fcitx5/{Core,Utils,Config},
#                   libraries libFcitx5Core.so / libFcitx5Utils.so
#   other distros : may use a plain `fcitx5` module or a flat layout.
#
# So we try, in order:
#   1. pkg-config module Fcitx5Core (then Fcitx5Utils/Fcitx5Config for the
#      sibling include dirs), falling back to module `fcitx5`;
#   2. a manual probe of the common header layouts;
#   3. explicit -DFCITX5_* overrides are honored wherever applicable.
# ---------------------------------------------------------------------------
set(FCITX5_FOUND FALSE)

find_package(PkgConfig QUIET)
if(PkgConfig_FOUND)
  pkg_check_modules(FCITX5_CORE QUIET IMPORTED_TARGET Fcitx5Core)
  if(NOT FCITX5_CORE_FOUND)
    pkg_check_modules(FCITX5_CORE QUIET IMPORTED_TARGET fcitx5)
  endif()
  if(FCITX5_CORE_FOUND)
    # Optional sibling modules: they carry the include dirs (and link flags)
    # for the fcitx-utils / fcitx-config headers used by the engine headers.
    pkg_check_modules(FCITX5_UTILS QUIET IMPORTED_TARGET Fcitx5Utils)
    pkg_check_modules(FCITX5_CONFIG QUIET IMPORTED_TARGET Fcitx5Config)
    set(FCITX5_FOUND TRUE)
  endif()
endif()

if(NOT FCITX5_FOUND)
  # ---- manual probe -------------------------------------------------------
  # Shape A (Debian/Ubuntu and upstream): <root>/Core/fcitx/inputmethodengine.h
  # Shape B (flat layout):                 <root>/fcitx/inputmethodengine.h
  set(_FCITX5_HEADER_ROOTS
      /usr/include/Fcitx5 /usr/local/include/Fcitx5
      /usr/include /usr/local/include)
  set(FCITX5_INCLUDE_DIRS "")

  foreach(_root ${_FCITX5_HEADER_ROOTS})
    if(EXISTS "${_root}/Core/fcitx/inputmethodengine.h")
      foreach(_sub Core Utils Config)
        if(EXISTS "${_root}/${_sub}")
          list(APPEND FCITX5_INCLUDE_DIRS "${_root}/${_sub}")
        endif()
      endforeach()
    elseif(EXISTS "${_root}/fcitx/inputmethodengine.h")
      list(APPEND FCITX5_INCLUDE_DIRS "${_root}")
    endif()
    if(FCITX5_INCLUDE_DIRS)
      break()
    endif()
  endforeach()

  if(FCITX5_INCLUDE_DIRS)
    set(_FCITX5_ARCH_LIBDIRS
        /usr/lib/x86_64-linux-gnu /usr/lib/aarch64-linux-gnu
        /usr/lib/arm-linux-gnueabihf /usr/lib/arm-linux-gnueabi
        /usr/lib/riscv64-linux-gnu /usr/lib/powerpc64le-linux-gnu
        /usr/lib/s390x-linux-gnu /usr/lib /usr/lib64
        /usr/local/lib /usr/local/lib64)
    find_library(FCITX5_CORE_LIBRARY
                 NAMES Fcitx5Core fcitx5core
                 HINTS ${FCITX5_CORE_LIBRARY}
                 PATHS ${_FCITX5_ARCH_LIBDIRS})
    find_library(FCITX5_UTILS_LIBRARY
                 NAMES Fcitx5Utils fcitx5utils
                 HINTS ${FCITX5_UTILS_LIBRARY}
                 PATHS ${_FCITX5_ARCH_LIBDIRS})
    if(FCITX5_CORE_LIBRARY)
      set(FCITX5_LIBRARIES ${FCITX5_CORE_LIBRARY})
      if(FCITX5_UTILS_LIBRARY)
        list(APPEND FCITX5_LIBRARIES ${FCITX5_UTILS_LIBRARY})
      endif()
      set(FCITX5_FOUND TRUE)
    endif()
  endif()
endif()

if(NOT FCITX5_FOUND)
  message(FATAL_ERROR
    "fcitx5 core not found. Install the fcitx5 development packages, e.g.\n"
    "  Debian/Ubuntu: sudo apt install libfcitx5core-dev libfcitx5utils-dev libfcitx5config-dev\n"
    "  Fedora:        sudo dnf install fcitx5-devel\n"
    "  Arch:          sudo pacman -S fcitx5\n"
    "or pass -DFCITX5_CORE_LIBRARY=/path/to/libFcitx5Core.so\n"
    "       -DFCITX5_UTILS_LIBRARY=/path/to/libFcitx5Utils.so")
endif()

# ---------------------------------------------------------------------------
# Engine addon
# ---------------------------------------------------------------------------
add_library(pskk SHARED
  src/pskkjson.cpp
  src/pskkclient.cpp
  src/pskkengine.cpp)
set_target_properties(pskk PROPERTIES
  CXX_STANDARD 17
  CXX_STANDARD_REQUIRED ON)
target_include_directories(pskk PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/src)

if(TARGET PkgConfig::FCITX5_CORE)
  target_link_libraries(pskk PRIVATE PkgConfig::FCITX5_CORE)
  # pkg-config 'Requires' usually brings the sibling dirs in already, but
  # append them explicitly when the sibling modules exist.
  if(TARGET PkgConfig::FCITX5_UTILS)
    target_link_libraries(pskk PRIVATE PkgConfig::FCITX5_UTILS)
    target_include_directories(pskk PRIVATE ${FCITX5_UTILS_INCLUDE_DIRS})
  endif()
  if(TARGET PkgConfig::FCITX5_CONFIG)
    target_include_directories(pskk PRIVATE ${FCITX5_CONFIG_INCLUDE_DIRS})
  endif()
else()
  target_include_directories(pskk PRIVATE ${FCITX5_INCLUDE_DIRS})
  target_link_libraries(pskk PRIVATE ${FCITX5_LIBRARIES})
endif()

# fcitx resolves an addon library by the exact name "Library" + ".so"
# (see fcitx5's addonloader.cpp: libname + FCITX_LIBRARY_SUFFIX), so the
# shared library must be named pskk.so -- NOT libpskk.so. This mirrors how
# upstream fcitx5-skk / fcitx5-mozc ship (skk.so, mozc.so, ...).
# Also hide non-exported symbols (fcitx addons only export the factory).
set_target_properties(pskk PROPERTIES CXX_VISIBILITY_PRESET hidden
                                      VISIBILITY_INLINES_HIDDEN ON
                                      OUTPUT_NAME pskk
                                      PREFIX "")

# ---------------------------------------------------------------------------
# Registration data (installed into the fcitx5 data directories)
# ---------------------------------------------------------------------------
# Addon metadata: tells fcitx5 the shared library is an input method addon.
# Installed as <datadir>/fcitx5/addon/pskk.conf.
configure_file(data/pskk-addon.conf.in ${CMAKE_CURRENT_BINARY_DIR}/pskk-addon.conf @ONLY)
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/pskk-addon.conf
        DESTINATION ${CMAKE_INSTALL_DATADIR}/fcitx5/addon
        RENAME pskk.conf)

# Input method entry: registers the "PSKK" input method (language ja) that
# this addon provides. Installed as <datadir>/fcitx5/inputmethod/pskk.conf.
configure_file(data/pskk.conf.in ${CMAKE_CURRENT_BINARY_DIR}/pskk.conf @ONLY)
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/pskk.conf
        DESTINATION ${CMAKE_INSTALL_DATADIR}/fcitx5/inputmethod)

install(TARGETS pskk LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}/fcitx5)

# Input-mode indicator icons (pskk-hiragana / pskk-alphanumeric) installed
# into the hicolor icon theme. After installing, refresh the icon cache with:
#   sudo gtk-update-icon-cache /usr/share/icons/hicolor
install(DIRECTORY data/icons/
        DESTINATION ${CMAKE_INSTALL_DATADIR}/icons)
