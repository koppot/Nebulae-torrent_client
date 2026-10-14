# Nebulae Linux Packaging and Verification Guide

This document details the Linux architecture, pinned build baseline, distribution support status, packaging formats, verification procedures, and installation instructions for **Nebulae** (Subtitle: **Torrent Client by Asian DVD Club**).

---

## 1. Scope and Architecture

- **Target Architecture**: Linux `x86_64` (AMD64).
- **Release Build Baseline**: **Ubuntu 22.04 LTS (Jammy Jellyfish)** pinned in CI.
- **C Library Baseline**: **GLIBC 2.35**.
- **Application Model**: Strictly native desktop binary. **No Docker or container runtime is required for end users.**

### Why Ubuntu 22.04 as the Build Baseline?
Compiling on modern Linux distributions (such as Ubuntu 24.04 or Fedora 40) dynamically links against GLIBC 2.38 or GLIBC 2.39. Binaries compiled against GLIBC 2.38+ fail immediately on Ubuntu 22.04 (GLIBC 2.35) and Debian 12 (GLIBC 2.36) with missing symbol errors (`GLIBC_2.38 not found`).

By pinning the official release build container to Ubuntu 22.04 LTS:
- The compiled binaries link against GLIBC 2.35.
- The resulting packages run natively across Ubuntu 22.04, Ubuntu 24.04, Debian 12+, Linux Mint 21+, Fedora 38+, openSUSE Leap 15.6+, and modern rolling releases without modification.

---

## 2. Platform Support Classification

In accordance with strict verification standards, automated Linux CI validation (e.g. headless Xvfb runs, package extraction tests, and container installs) is **not described as equivalent to verified end-user Linux application support**. True verification requires testing under physical desktop compositors, native audio/tray servers, and network transfer paths.

Each target platform is classified according to the following taxonomy:

| Platform / Distribution | Minimum Version | Package Type | Status | Notes |
| :--- | :--- | :--- | :--- | :--- |
| **Ubuntu Desktop** | 22.04 LTS | `.deb`, `.AppImage` | **Launch tested** | Clean install, Xvfb launch, XDG init, shutdown, and uninstall verified on build runner. |
| **Ubuntu Desktop** | 24.04 LTS | `.deb`, `.AppImage` | **Installation tested** | Package dependency resolution and installation verified in container test suite. |
| **Debian GNU/Linux** | 12 (Bookworm) | `.deb`, `.AppImage` | **Installation tested** | Clean package install and removal verified in multi-distro test suite. |
| **Linux Mint** | 21 (Vanessa) / 22 | `.deb`, `.AppImage` | **Installation tested** | Shares Ubuntu/Debian package baseline and GTK3/Cinnamon desktop stack. |
| **Fedora Workstation** | 40+ | `.rpm` | **Installation tested** | RPM metadata, architecture, and DNF install verification tested. |
| **openSUSE Leap** | 15.6 / Tumbleweed | `.rpm` | **Installation tested** | RPM dependency compatibility and Zypper installation tested. |
| **Arch Linux / Manjaro** | Rolling | `.AppImage`, Standalone | **Untested** | Untested in container suite; portable AppImage or binary recommended. |
| **Linux 32-bit (i686)** | Any | N/A | **Unsupported** | 32-bit x86 is not supported; modern WebKitGTK and Rust targets require 64-bit. |
| **Linux ARM64 (aarch64)**| Any | N/A | **Untested** | Architecture build pipeline currently scoped to x86_64. |

---

## 3. Shipped Linux Packages

Every release publishes four official Linux artifacts:

1. **Debian Package (`Nebulae-Linux-x86_64.deb`)**:
   - Native Debian/Ubuntu package.
   - Installs binary to `/usr/bin/nebulae`.
   - Installs `.desktop` entry to `/usr/share/applications/nebulae.desktop`.
   - Installs multi-resolution icons to `/usr/share/icons/hicolor/...`.
   - Declared dependencies:
     - `libwebkit2gtk-4.1-0 | libwebkit2gtk-4.0-37`
     - `libgtk-3-0`
     - `libayatana-appindicator3-1`
     - `librsvg2-2`
     - `libssl3 | libssl1.1`

2. **RPM Package (`Nebulae-Linux-x86_64.rpm`)**:
   - Native RPM package for Fedora, RHEL, CentOS Stream, and openSUSE.
   - Declared dependencies:
     - `webkit2gtk4.1`
     - `gtk3`
     - `libayatana-appindicator3`
     - `librsvg2`
     - `openssl`

3. **Portable AppImage (`Nebulae-Linux-x86_64.AppImage`)**:
   - Self-contained, portable single-file executable.
   - Requires no package manager privileges.
   - Bundles core GTK and WebKit libraries.

4. **Standalone Native Binary (`nebulae-linux-x86_64`)**:
   - Raw ELF executable for power users and custom distributions.

---

## 4. Installation Walkthrough

### Debian, Ubuntu, and Linux Mint (`.deb`)

#### Recommended (APT with automatic dependency resolution):
```bash
sudo apt update
sudo apt install ./Nebulae-Linux-x86_64.deb
```

#### Alternative (dpkg):
```bash
sudo dpkg -i Nebulae-Linux-x86_64.deb
sudo apt-get install -f -y
```

---

### Fedora and RHEL-compatible Distributions (`.rpm`)

```bash
sudo dnf install ./Nebulae-Linux-x86_64.rpm
```

---

### openSUSE Leap & Tumbleweed (`.rpm`)

```bash
sudo zypper --non-interactive install --allow-unsigned-rpm ./Nebulae-Linux-x86_64.rpm
```

---

### Portable AppImage

1. Make the AppImage executable:
   ```bash
   chmod +x Nebulae-Linux-x86_64.AppImage
   ```
2. Run the application:
   ```bash
   ./Nebulae-Linux-x86_64.AppImage
   ```

> [!NOTE]
> **FUSE Compatibility**: Modern minimalist distributions (such as Ubuntu 24.04 default desktop) do not ship `libfuse2` by default. If the AppImage fails to launch with a FUSE error, install the compatibility library:
> ```bash
> sudo apt install libfuse2t64  # Ubuntu 24.04+ / Debian 12
> # or sudo apt install libfuse2  # Ubuntu 22.04
> ```
> Alternatively, execute using extracted mode:
> ```bash
> ./Nebulae-Linux-x86_64.AppImage --appimage-extract-and-run
> ```

---

## 5. Desktop and System Integration

### System Tray & AppIndicator
Nebulae integrates with the system notification area using `libayatana-appindicator3-1`. On GNOME Desktop environments, ensure the **AppIndicator and KStatusNotifierItem Support** GNOME Shell extension is enabled if your distribution does not ship it by default.

### Native File & Folder Dialogs
Nebulae uses native file picker backends:
- **GNOME / XFCE / MATE / Cinnamon**: Uses `zenity` if present for native GTK file selection.
- **KDE Plasma**: Uses `kdialog` for native Qt/KDE file selection.
- **Fallback**: Built-in Tauri file system dialogs.

### MIME Associations and Magnet Links
The package registers:
- `application/x-bittorrent` for `.torrent` files
- `application/x-adc` for `.adc` files
- `x-scheme-handler/magnet` for `magnet:?xt=...` browser links

### External Links
Community links (such as `asiandvdclub.org`) invoke `xdg-open` to respect the user's configured default browser.

---

## 6. Display Server Support (X11 and Wayland)

Nebulae supports both **X11** and **Wayland**:
- **X11**: Fully supported natively across all desktop compositors.
- **Wayland**: Supported via GTK3 Wayland backend. If fractional scaling or compositor quirks occur under Wayland, force the X11 backend by launching with:
  ```bash
  GDK_BACKEND=x11 nebulae
  ```
  To force native Wayland:
  ```bash
  GDK_BACKEND=wayland nebulae
  ```

---

## 7. Isolated Internal Storage (XDG Compliance)

Nebulae strictly complies with the **XDG Base Directory Specification**. User data and runtime files are isolated completely outside package directories:

```text
~/.local/share/Nebulae/internal/
├── db/             # SQLite metadata database (nebulae.db)
├── logs/           # Rotated session logs (nebulae_session.log)
├── cache/          # Fast resume state & piece cache
├── engine-state/   # BitTorrent engine checkpoints
└── diagnostics/    # Crash and panic reports
```

Settings are persisted in `~/.config/ADC/` or `~/.local/share/Nebulae/internal/`.

---

## 8. Uninstallation

### Debian / Ubuntu:
```bash
sudo dpkg -r nebulae
sudo dpkg --purge nebulae
```

### Fedora:
```bash
sudo dnf remove nebulae
```

### openSUSE:
```bash
sudo zypper remove nebulae
```

### Clean Purge (Remove User Torrent Data & Logs):
```bash
rm -rf ~/.local/share/Nebulae
rm -rf ~/.config/ADC
```

---

## 9. Integrity Verification

Always verify your downloaded packages against the official `SHA256SUMS.txt` published with each release:

```bash
sha256sum -c Nebulae-Linux-x86_64.deb.sha256
# or
sha256sum --ignore-missing -c SHA256SUMS.txt
```
