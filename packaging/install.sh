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

# Build the project first (skip if running with sudo, assume pre-built)
if [[ $EUID -eq 0 ]]; then
    echo "Running as root, skipping build step..."
    echo "Please ensure binaries are pre-built with: just build-all"
    echo ""
else
    echo "Building PSKK..."
    if command -v just &> /dev/null; then
        just build-all
    else
        echo "Error: 'just' is required but not installed."
        echo "Please install just: https://github.com/casey/just"
        exit 1
    fi
    echo ""
fi

# Create directory structure
echo "Creating directories..."
mkdir -p "${PSKK_BIN_DIR}"
mkdir -p "${PSKK_LIB_DIR}"
mkdir -p "${PSKK_LIBEXEC_DIR}"
mkdir -p "${PSKK_DATA_DIR}"
mkdir -p "${PSKK_IBUS_COMPONENT_DIR}"

# Install binaries
echo "Installing binaries..."
if [ -f "apps/settings/src-tauri/target/release/pskk-settings" ]; then
    cp apps/settings/src-tauri/target/release/pskk-settings "${PSKK_BIN_DIR}/"
    chmod +x "${PSKK_BIN_DIR}/pskk-settings"
    echo "  ✓ pskk-settings"
else
    echo "  ⚠ pskk-settings not found (build with: just settings-tauri-build)"
fi

if [ -f "apps/ime-tester/src-tauri/target/release/pskk-ime-tester" ]; then
    cp apps/ime-tester/src-tauri/target/release/pskk-ime-tester "${PSKK_BIN_DIR}/"
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
    # Make data files readable by all users
    chmod -R a+rX "${PSKK_DATA_DIR}"
    echo "  ✓ Data files"
fi

# Download SKK dictionaries (not included in packages due to license)
echo "Downloading SKK dictionaries to ${PSKK_DATA_DIR}/skk_dict/..."
if command -v curl &> /dev/null; then
    if command -v iconv &> /dev/null; then
        mkdir -p "${PSKK_DATA_DIR}/skk_dict"
        for file in SKK-JISYO.L SKK-JISYO.M SKK-JISYO.ML SKK-JISYO.S; do
            echo "  Downloading $file..."
            if curl -f -L -o "${PSKK_DATA_DIR}/skk_dict/$file" https://raw.githubusercontent.com/skk-dev/dict/master/$file; then
                echo "    ✓ $file downloaded"
                # Convert from EUC-JP to UTF-8
                echo "    Converting $file from EUC-JP to UTF-8..."
                if iconv -f EUC-JP -t UTF-8 "${PSKK_DATA_DIR}/skk_dict/$file" > "${PSKK_DATA_DIR}/skk_dict/$file.utf8"; then
                    mv "${PSKK_DATA_DIR}/skk_dict/$file.utf8" "${PSKK_DATA_DIR}/skk_dict/$file"
                    echo "    ✓ $file converted to UTF-8"
                else
                    echo "    ⚠ Failed to convert $file (keeping original encoding)"
                    rm -f "${PSKK_DATA_DIR}/skk_dict/$file.utf8"
                fi
            else
                echo "    ⚠ Failed to download $file (will be skipped)"
            fi
        done
        # Make dictionaries readable by all users
        chmod -R a+rX "${PSKK_DATA_DIR}/skk_dict"
        echo "  ✓ SKK dictionaries"
    else
        echo "  ✗ iconv not found, cannot convert SKK dictionaries to UTF-8"
        echo "  Please install iconv and run the script again"
        exit 1
    fi
else
    echo "  ✗ curl not found, cannot download SKK dictionaries"
    echo "  Please install curl and run the script again"
    exit 1
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

# Install Fcitx 5 addon (if built with: just fcitx5-build)
if [ -d "fcitx5/build" ] && [ -f "fcitx5/build/pskk.so" ]; then
    echo "Installing Fcitx 5 addon..."
    cmake --install fcitx5/build > /dev/null
    echo "  ✓ Fcitx 5 addon installed (restart fcitx5 to activate)"
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
echo "  For Fcitx 5: restart fcitx5 (fcitx5-remote -r) and add PSKK"
echo "  in fcitx5-configtool → Add Input Method"
echo ""
echo "To uninstall, run: sudo ${SCRIPT_DIR}/uninstall.sh"
