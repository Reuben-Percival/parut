#  Parut

<div align="center">

**A beautiful graphical frontend for the Paru AUR helper**

[![License: GPL-3.0](https://img.shields.io/badge/License-GPL%203.0-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)
[![GTK4](https://img.shields.io/badge/GTK-4-green.svg)](https://gtk.org/)
[![libadwaita](https://img.shields.io/badge/libadwaita-1.5-purple.svg)](https://gnome.pages.gitlab.gnome.org/libadwaita/)

</div>

---

## ✨ Features

-  Modern UI - Built with GTK4 and libadwaita for a beautiful, native GNOME experience
- Package Search- Search packages from official Arch repositories and the AUR with debounced, fast search
- Installed Packages - View and manage all your installed packages with easy filtering
- System Update - Check for and apply system updates with one click
- Task Queue - Queue multiple package operations and monitor their progress
- PKGBUILD Review - Review AUR package build scripts before installation for security
- Dashboard- See system stats at a glance including installed packages, AUR packages, and available updates


##  Prerequisites

Before installing Parut, ensure you have:

- **Arch Linux** (or an Arch-based distribution)
- **Paru** - The AUR helper ([installation guide](https://github.com/Morganamilo/paru))
- **GTK4** and **libadwaita** development libraries
- **Rust** toolchain (1.70+)

### Installing Dependencies

```bash
# Install paru (if not already installed)
sudo pacman -S --needed base-devel
git clone https://aur.archlinux.org/paru.git
cd paru
makepkg -si

# Install GTK4 and libadwaita
sudo pacman -S gtk4 libadwaita
```

##  Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/Reuben-Percival/parut.git
cd parut

# Build and install
cargo build --release

# Run the installer
chmod +x install.sh
./install.sh
```

### Manual Installation

```bash
# Build
cargo build --release

# Copy to a location in your PATH
sudo cp target/release/parut /usr/local/bin/
```

## 🚀 Usage

Simply run:

```bash
parut
```

Or launch from your application menu after installation.

### Navigation

- **Overview** - Dashboard showing system statistics and quick actions
- **Search** - Search for packages in repos and AUR (type at least 2 characters)
- **Installed** - View and manage all installed packages
- **Updates** - Check for and apply available updates

### Actions

- **Install** - Click the + button on any package row to install
- **Remove** - Click the trash icon to remove installed packages
- **Update All** - Click "Update All" in the Updates tab to update your system
- **Refresh** - Click the Refresh button to reload package lists
- **Queue** - View ongoing package operations in the task queue

## ⚙️ Configuration

Parut respects your system's dark/light theme preference through libadwaita's style manager. The application automatically adapts to your system theme.

Logs are saved to:
```
~/.local/share/parut/parut.log
```

## 🔒 Security

When installing AUR packages, Parut will show you the PKGBUILD for review before proceeding. **Always review PKGBUILDs** for potentially malicious content before installation. DO NOT INSTALL STUFF THAT IS TO GOOD TO BE TRUE AKA firefox-patched-ultrafps

## 🤝 Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📋 Roadmap

- [ ] Package cache cleaning
- [ ] Orphan package removal
- [ ] Package details view with dependencies
- [ ] Configuration UI
- [ ] System notifications for updates
- [ ] Package sorting and filtering options
- [ ] Batch selection for actions

## 📜 License

This project is licensed under the GPL-3.0 License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments ( support these guys not me)

- [Paru](https://github.com/Morganamilo/paru) - The amazing AUR helper that powers this application
- [GTK4](https://gtk.org/) - The cross-platform widget toolkit
- [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/) - For the beautiful adaptive UI components

---

<div align="center">
Made with ❤️ for Arch Linux users
</div>
