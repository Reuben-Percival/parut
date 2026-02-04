#!/bin/bash

# Parut Installation Script
set -e

echo "==================================="
echo "Installing Parut AUR Package Manager"
echo "==================================="

# Check if running as root
if [ "$EUID" -eq 0 ]; then 
    echo "Error: Please do not run this script as root"
    exit 1
fi

# Check if paru is installed
if ! command -v paru &> /dev/null; then
    echo "Warning: paru is not installed. Parut requires paru to function."
    echo "You can install paru from the AUR first."
    read -p "Continue anyway? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    echo "Error: cargo is not installed. Please install Rust first."
    echo "Visit https://rustup.rs/ or run: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

echo "Building Parut..."
cargo build --release

if [ $? -ne 0 ]; then
    echo "Error: Build failed"
    exit 1
fi

echo "Installing binary..."
sudo install -Dm755 target/release/parut /usr/local/bin/parut

# Create desktop entry
echo "Creating desktop entry..."
sudo mkdir -p /usr/share/applications

cat << EOF | sudo tee /usr/share/applications/parut.desktop > /dev/null
[Desktop Entry]
Name=Parut
Comment=AUR Package Manager GUI
Exec=parut
Icon=system-software-install
Terminal=false
Type=Application
Categories=System;PackageManager;
Keywords=package;aur;paru;
EOF

# Create icon directory if it doesn't exist
sudo mkdir -p /usr/share/icons/hicolor/scalable/apps

echo ""
echo "==================================="
echo "Installation completed successfully!"
echo "==================================="
echo ""
echo "You can now run Parut by:"
echo "  1. Running 'parut' from the terminal"
echo "  2. Searching for 'Parut' in your application menu"
echo ""
echo "Note: Make sure paru is installed for full functionality"
echo "==================================="
