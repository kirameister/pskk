#!/bin/bash
# PSKK Uninstallation Script

set -e

# Source configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/install-config.sh"

# Check if running as root for system uninstallation
if [[ $EUID -ne 0 ]] && [[ "${PSKK_PREFIX}" == /opt/* || "${PSKK_PREFIX}" == /usr/* ]]; then
   echo "Error: System uninstallation requires root privileges"
   echo "Please run with sudo: sudo $0"
   exit 1
fi

echo "Uninstalling PSKK..."
print_config
echo ""

# Remove symlinks
echo "Removing symlinks..."
rm -f "${SYSTEM_BIN_DIR}/pskk-settings"
rm -f "${SYSTEM_BIN_DIR}/pskk-ime-tester"
echo "  ✓ Symlinks removed"

# Remove IBus component
echo "Removing IBus component..."
rm -f "${SYSTEM_IBUS_COMPONENT_DIR}/pskk.xml"
echo "  ✓ IBus component removed"

# Remove desktop files
echo "Removing desktop files..."
rm -f "${SYSTEM_APPLICATIONS_DIR}/pskk-settings.desktop"
echo "  ✓ Desktop files removed"

# Remove installation directory
echo "Removing ${PSKK_PREFIX}..."
rm -rf "${PSKK_PREFIX}"
echo "  ✓ Installation directory removed"

echo ""
echo "✓ Uninstallation complete!"
echo ""
echo "Note: User configuration in ${PSKK_USER_CONFIG_DIR} was preserved."
echo "To remove user data, run: rm -rf ${PSKK_USER_CONFIG_DIR}"
echo ""
echo "Run 'ibus restart' to complete IBus unregistration."
