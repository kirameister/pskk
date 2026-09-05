#!/bin/bash
# PSKK Installation Configuration
# This file defines installation paths and can be sourced by install scripts

# Installation prefix (can be overridden via environment variable)
export PSKK_PREFIX="${PSKK_PREFIX:-/opt/pskk}"

# Directory structure under $PSKK_PREFIX
export PSKK_BIN_DIR="${PSKK_PREFIX}/bin"
export PSKK_LIB_DIR="${PSKK_PREFIX}/lib"
export PSKK_LIBEXEC_DIR="${PSKK_PREFIX}/libexec"
export PSKK_DATA_DIR="${PSKK_PREFIX}/data"
export PSKK_IBUS_COMPONENT_DIR="${PSKK_PREFIX}/share/ibus/component"

# System integration paths
export SYSTEM_BIN_DIR="/usr/local/bin"
export SYSTEM_IBUS_COMPONENT_DIR="/usr/share/ibus/component"
export SYSTEM_APPLICATIONS_DIR="/usr/share/applications"

# Fcitx 5 integration paths (Debian multiarch / Fedora lib64 / plain lib)
export SYSTEM_FCITX5_ADDON_DIR="/usr/share/fcitx5/addon"
export SYSTEM_FCITX5_INPUTMETHOD_DIR="/usr/share/fcitx5/inputmethod"
_FCITX5_MULTIARCH="$(dpkg-architecture -qDEB_HOST_MULTIARCH 2>/dev/null || true)"
if [ -n "${_FCITX5_MULTIARCH}" ]; then
    export SYSTEM_FCITX5_LIB_DIR="/usr/lib/${_FCITX5_MULTIARCH}/fcitx5"
elif [ -d "/usr/lib64" ]; then
    export SYSTEM_FCITX5_LIB_DIR="/usr/lib64/fcitx5"
else
    export SYSTEM_FCITX5_LIB_DIR="/usr/lib/fcitx5"
fi
unset _FCITX5_MULTIARCH

# User configuration directory
export PSKK_USER_CONFIG_DIR="${HOME}/.config/pskk"

# Print configuration
print_config() {
    echo "PSKK Installation Configuration:"
    echo "  PREFIX: ${PSKK_PREFIX}"
    echo "  BIN_DIR: ${PSKK_BIN_DIR}"
    echo "  LIB_DIR: ${PSKK_LIB_DIR}"
    echo "  LIBEXEC_DIR: ${PSKK_LIBEXEC_DIR}"
    echo "  DATA_DIR: ${PSKK_DATA_DIR}"
    echo "  USER_CONFIG_DIR: ${PSKK_USER_CONFIG_DIR}"
}

# Usage example:
# source packaging/install-config.sh
# print_config
