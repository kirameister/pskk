cmake_minimum_required(VERSION 3.16)
project(pskk-fcitx5 VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

include(GNUInstallDirs)

# ---------------------------------------------------------------------------
# Locate fcitx5 core (headers + library). Preferred route: pkg-config, which
# the libfcitx5core-dev package ships. Fall back to manual search and honor
# FCITX5_INCLUDE_DIR / FCITX5_LIBRARY overrides.
# ---------------------------------------------------------------------------
find_package(PkgConfig QUIET)
if(PkgConfig_FOUND)
  pkg_check_modules(FCITX5 QUIET IMPORTED_TARGET fcitx5)
endif()

if(NOT FCITX5_FOUND)
  find_path(FCITX5_INCLUDE_DIR fcitx/inputmethodengine.h
            HINTS ${FCITX5_INCLUDE_DIR}
            PATHS /usr/include/fcitx5 /usr/local/include/fcitx5
                  /usr/include
            PATH_SUFFIXES fcitx5)
  find_library(FCITX5_LIBRARY NAMES fcitx5core
               HINTS ${FCITX5_LIBRARY}
               PATHS /usr/lib /usr/lib64 /usr/local/lib
                     /usr/lib/x86_64-linux-gnu /usr/lib/aarch64-linux-gnu)
  if(FCITX5_INCLUDE_DIR AND FCITX5_LIBRARY)
    set(FCITX5_FOUND TRUE)
    set(FCITX5_INCLUDE_DIRS ${FCITX5_INCLUDE_DIR})
    set(FCITX5_LIBRARIES ${FCITX5_LIBRARY})
  endif()
endif()

if(NOT FCITX5_FOUND)
  message(FATAL_ERROR
    "fcitx5 core not found. Install libfcitx5core-dev (Debian/Ubuntu), "
    "or pass -DFCITX5_INCLUDE_DIR=... -DFCITX5_LIBRARY=...")
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

if(FCITX5_FOUND AND TARGET PkgConfig::FCITX5)
  target_link_libraries(pskk PRIVATE PkgConfig::FCITX5)
else()
  target_include_directories(pskk PRIVATE ${FCITX5_INCLUDE_DIRS})
  target_link_libraries(pskk PRIVATE ${FCITX5_LIBRARIES})
endif()

# Hide non-exported symbols (fcitx addons should only export the factory).
set_target_properties(pskk PROPERTIES CXX_VISIBILITY_PRESET hidden
                                      VISIBILITY_INLINES_HIDDEN ON)

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
