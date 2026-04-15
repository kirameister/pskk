#!/bin/bash
# PSKK Installation Script
# Installs PSKK to /opt/pskk by default

set -e

# Source configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/install-config.sh"

# Check if running as root for system installation
if [[ $EUID -ne 0 ]] && [[ "${PSKK_PREFIX}" == /opt/* || "${PSKK_PREFIX}" == /usr/* ]]; then
   echo "Error: System installation requires root privileges"
   echo "Please run with sudo: sudo $0"
   exit 1
fi

echo "Installing PSKK..."
print_config
echo ""

# Create directory structure
echo "Creating directories..."
mkdir -p "${PSKK_BIN_DIR}"
mkdir -p "${PSKK_LIB_DIR}"
mkdir -p "${PSKK_LIBEXEC_DIR}"
mkdir -p "${PSKK_DATA_DIR}"
mkdir -p "${PSKK_IBUS_COMPONENT_DIR}"

# Install binaries
echo "Installing binaries..."
if [ -f "target/release/pskk-settings" ]; then
    cp target/release/pskk-settings "${PSKK_BIN_DIR}/"
    chmod +x "${PSKK_BIN_DIR}/pskk-settings"
    echo "  ✓ pskk-settings"
else
    echo "  ⚠ pskk-settings not found (build with: just settings-tauri-build)"
fi

if [ -f "target/release/pskk-ime-tester" ]; then
    cp target/release/pskk-ime-tester "${PSKK_BIN_DIR}/"
    chmod +x "${PSKK_BIN_DIR}/pskk-ime-tester"
    echo "  ✓ pskk-ime-tester"
else
    echo "  ⚠ pskk-ime-tester not found (build with: just ime-tester-build)"
fi

if [ -f "target/release/ibus-engine-pskk" ]; then
    cp target/release/ibus-engine-pskk "${PSKK_LIBEXEC_DIR}/"
    chmod +x "${PSKK_LIBEXEC_DIR}/ibus-engine-pskk"
    echo "  ✓ ibus-engine-pskk"
else
    echo "  ⚠ ibus-engine-pskk not found (not yet implemented)"
fi

# Install libraries
echo "Installing libraries..."
if [ -f "target/release/libpskk.rlib" ]; then
    cp target/release/libpskk.rlib "${PSKK_LIB_DIR}/"
    echo "  ✓ libpskk.rlib"
fi

if [ -f "target/release/libpskk.so" ]; then
    cp target/release/libpskk.so "${PSKK_LIB_DIR}/"
    echo "  ✓ libpskk.so"
fi

# Install data files (dictionaries, models, etc.)
echo "Installing data files..."
if [ -d "data" ]; then
    cp -r data/* "${PSKK_DATA_DIR}/" 2>/dev/null || true
    echo "  ✓ Data files"
fi

# Create symlinks in /usr/local/bin for easy access
if [[ "${PSKK_PREFIX}" != "${SYSTEM_BIN_DIR}" ]]; then
    echo "Creating symlinks in ${SYSTEM_BIN_DIR}..."
    mkdir -p "${SYSTEM_BIN_DIR}"
    
    if [ -f "${PSKK_BIN_DIR}/pskk-settings" ]; then
        ln -sf "${PSKK_BIN_DIR}/pskk-settings" "${SYSTEM_BIN_DIR}/pskk-settings"
        echo "  ✓ pskk-settings -> ${SYSTEM_BIN_DIR}/pskk-settings"
    fi
    
    if [ -f "${PSKK_BIN_DIR}/pskk-ime-tester" ]; then
        ln -sf "${PSKK_BIN_DIR}/pskk-ime-tester" "${SYSTEM_BIN_DIR}/pskk-ime-tester"
        echo "  ✓ pskk-ime-tester -> ${SYSTEM_BIN_DIR}/pskk-ime-tester"
    fi
fi

# Install IBus component file (if exists)
if [ -f "packaging/pskk.xml" ]; then
    echo "Installing IBus component..."
    cp packaging/pskk.xml "${PSKK_IBUS_COMPONENT_DIR}/"
    # Also install to system IBus location
    if [ -d "${SYSTEM_IBUS_COMPONENT_DIR}" ]; then
        cp packaging/pskk.xml "${SYSTEM_IBUS_COMPONENT_DIR}/"
        echo "  ✓ IBus component installed"
    fi
fi

# Install desktop files
if [ -f "packaging/pskk-settings.desktop" ]; then
    echo "Installing desktop files..."
    mkdir -p "${SYSTEM_APPLICATIONS_DIR}"
    cp packaging/pskk-settings.desktop "${SYSTEM_APPLICATIONS_DIR}/"
    echo "  ✓ Desktop entry installed"
fi

echo ""
echo "✓ Installation complete!"
echo ""
echo "Installation location: ${PSKK_PREFIX}"
echo ""
echo "Next steps:"
echo "  1. Run 'pskk-settings' to configure PSKK"
echo "  2. Run 'ibus restart' to register the IBus engine"
echo "  3. Add PSKK in IBus preferences (ibus-setup)"
echo ""
echo "To uninstall, run: sudo ${SCRIPT_DIR}/uninstall.sh"
