#!/bin/bash

# Parut Uninstallation Script
set -e

echo "====================================="
echo "Uninstalling Parut AUR Package Manager"
echo "====================================="

# Check if running as root
if [ "$EUID" -eq 0 ]; then 
    echo "Error: Please do not run this script as root"
    exit 1
fi

# Confirm uninstallation
read -p "Are you sure you want to uninstall Parut? (y/N): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Uninstallation cancelled."
    exit 0
fi

echo "Removing binary..."
if [ -f "/usr/local/bin/parut" ]; then
    sudo rm -f /usr/local/bin/parut
    echo "✓ Removed /usr/local/bin/parut"
else
    echo "⚠ Binary not found at /usr/local/bin/parut"
fi

echo "Removing desktop entry..."
if [ -f "/usr/share/applications/parut.desktop" ]; then
    sudo rm -f /usr/share/applications/parut.desktop
    echo "✓ Removed /usr/share/applications/parut.desktop"
else
    echo "⚠ Desktop entry not found"
fi

# Update desktop database
if command -v update-desktop-database &> /dev/null; then
    echo "Updating desktop database..."
    sudo update-desktop-database /usr/share/applications/ 2>/dev/null || true
fi

echo ""
echo "====================================="
echo "Uninstallation completed successfully!"
echo "====================================="
echo ""
echo "Note: Build artifacts in the project directory (target/) were not removed."
echo "You can manually delete the project directory if desired."
echo "====================================="
