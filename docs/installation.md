# Nebulae Installation and User Guide

This guide provides step-by-step, beginner-friendly instructions for installing, configuring, and updating **Nebulae** (Subtitle: **Torrent Client by Asian DVD Club**) on Windows, macOS, and Linux.

---

## Table of Contents

1. [Understanding Nebulae](#understanding-nebulae)
2. [Windows x64 Installation (NSIS Installer)](#1-windows-x64-installation-nsis-installer)
   - [System Requirements](#windows-system-requirements)
   - [Downloading Nebulae-Setup-x64.exe](#downloading-nebulae-setup-x64exe)
   - [Windows SmartScreen & Antivirus Notice](#windows-smartscreen--antivirus-notice)
   - [Step-by-Step Installation Walkthrough](#step-by-step-installation-walkthrough)
   - [First Launch & Windows Firewall Configuration](#first-launch--windows-firewall-configuration)
   - [Adding Your First Torrent](#adding-your-first-torrent-windows)
   - [Updating Nebulae on Windows](#updating-nebulae-on-windows)
   - [Uninstalling Nebulae on Windows](#uninstalling-nebulae-on-windows)
   - [Windows Troubleshooting & Log Locations](#windows-troubleshooting)
3. [macOS Apple Silicon Installation (M1, M2, M3, M4, M5)](#2-macos-apple-silicon-installation)
   - [Confirming Apple Silicon](#confirming-apple-silicon)
   - [Downloading Nebulae-macOS-arm64.dmg](#downloading-nebulae-macos-arm64dmg)
   - [Step-by-Step Installation Walkthrough](#step-by-step-macos-installation)
   - [macOS Gatekeeper Verification](#macos-gatekeeper-verification)
   - [First Launch & Permissions](#first-launch--permissions-macos)
   - [Adding Your First Torrent](#adding-your-first-torrent-macos)
   - [Updating Nebulae on macOS](#updating-nebulae-on-macos)
   - [Uninstalling Nebulae on macOS](#uninstalling-nebulae-on-macos)
   - [macOS Troubleshooting & Log Locations](#macos-troubleshooting)
4. [macOS Intel Installation (x86_64)](#3-macos-intel-installation-x86_64)
   - [Confirming an Intel Mac](#confirming-an-intel-mac)
   - [Downloading Nebulae-macOS-x64.dmg](#downloading-nebulae-macos-x64dmg)
   - [Installation & Gatekeeper Walkthrough](#installation--gatekeeper-intel)
5. [Linux x86_64 Installation (.deb, .rpm, .AppImage)](#4-linux-x86_64-installation-deb-rpm-appimage)
   - [Supported Distributions & Baseline](#linux-supported-distributions)
   - [Debian, Ubuntu & Linux Mint (.deb)](#debian-ubuntu-and-linux-mint-deb)
   - [Fedora & Red Hat (.rpm)](#fedora-and-red-hat-rpm)
   - [openSUSE (.rpm)](#opensuse-rpm)
   - [Portable AppImage](#portable-appimage)
   - [Linux Desktop Integration & File Pickers](#linux-desktop-integration)
   - [Uninstalling on Linux](#uninstalling-on-linux)
6. [Isolated Internal Storage & Diagnostics](#5-isolated-internal-storage--diagnostics)

---

## Understanding Nebulae

- **Product Name**: **Nebulae**
- **Product Subtitle**: **Torrent Client by Asian DVD Club**
- **Executable Name**: **`nebulae`** (`nebulae.exe` on Windows)
- **macOS Application Bundle**: **`Nebulae.app`**
- **Installers & Disk Images**:
  - Windows: **`Nebulae-Setup-x64.exe`**
  - macOS Apple Silicon: **`Nebulae-macOS-arm64.dmg`**
  - macOS Intel: **`Nebulae-macOS-x64.dmg`**
  - Linux Debian/Ubuntu: **`Nebulae-Linux-x86_64.deb`**
  - Linux Red Hat/Fedora/openSUSE: **`Nebulae-Linux-x86_64.rpm`**
  - Linux Portable: **`Nebulae-Linux-x86_64.AppImage`**
  - Linux Raw Executable: **`nebulae-linux-x86_64`**
- **Startup Splash Screen**: On launch, Nebulae presents a startup splash screen displaying internal initialization progress and a community invitation to visit `asiandvdclub.org`. The splash screen can be skipped immediately or disabled in Settings.
- **Isolated Technical Files**: Database files, session logs, engine state, and caches are isolated in an internal directory (`Nebulae/internal/`) and never exposed in the standard interface.

---

## 1. Windows x64 Installation (NSIS Installer)

### Windows System Requirements

| Specification | Requirement |
| :--- | :--- |
| **Operating System** | Windows 10 (64-bit) or Windows 11 (64-bit) |
| **Processor** | 64-bit Intel or AMD processor |
| **Memory (RAM)** | 2 GB minimum (4 GB recommended) |
| **Disk Storage** | 120 MB free space for application + storage for downloaded media |
| **Network** | Broadband internet connection |

### Downloading Nebulae-Setup-x64.exe

1. Navigate to the official releases repository: `https://github.com/koppot/Nebulae-torrent_client/releases`
2. Under the latest release header, locate the **Assets** section.
3. Download **`Nebulae-Setup-x64.exe`**.

#### Code signing policy

> **SignPath Foundation application pending; signing is not yet enabled.** Windows artifacts are currently **unsigned** beta builds; SmartScreen will warn about an unrecognized publisher. See the [Code signing policy](code-signing-policy.md) for the policy that will govern signed releases. Once accepted: Free code signing provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).

### Windows SmartScreen & Antivirus Notice

When running newly released open-source software on Windows, **Windows Defender SmartScreen** or third-party antivirus utilities may display a warning dialog stating:
> *"Windows protected your PC — Microsoft Defender SmartScreen prevented an unrecognized app from starting."*

#### How to Proceed Safely:
1. Click the text link labeled **"More info"**.
2. A button labeled **"Run anyway"** will appear in the bottom-right corner.
3. Click **"Run anyway"** to launch the installer.

---

### Step-by-Step Installation Walkthrough

1. Double-click the downloaded **`Nebulae-Setup-x64.exe`** file.
2. Select your language (**English**) and click **OK**.
3. Choose your destination directory (default is `C:\Program Files\Nebulae` or `%LOCALAPPDATA%\Programs\Nebulae`).
4. Choose whether to create a Desktop Shortcut and Start Menu icon.
5. Click **Install**.
6. When the installation finishes, check **"Launch Nebulae"** and click **Finish**.

---

### First Launch & Windows Firewall Configuration

When Nebulae launches for the first time, Windows Defender Firewall will prompt you to allow network access:

1. Check both **Private networks** (home or work) and **Public networks**.
2. Click **Allow access**.
3. If this dialog does not appear, open **Windows Defender Firewall** → **Allow an app through firewall** → verify that `nebulae.exe` is checked for both Private and Public networks.

---

### Adding Your First Torrent (Windows)

- **Magnet Links**: Click any `magnet:?xt=...` link in your web browser. Nebulae will handle the protocol.
- **Torrent Files**: Drag and drop any `.torrent` file directly onto the Nebulae window.
- **Manual Input**: Click **+ Add Torrent** in the top title bar to paste magnet links or inspect file details.

---

### Updating Nebulae on Windows

1. Download the latest **`Nebulae-Setup-x64.exe`** from GitHub Releases.
2. Run the installer. It will automatically detect your existing installation, update the application files, and preserve your database, settings, and active torrent transfers.

---

### Uninstalling Nebulae on Windows

1. Open **Windows Settings** (Win + I) → **Apps** → **Installed apps**.
2. Find **Nebulae** in the list.
3. Click the three dots (...) and select **Uninstall**.
4. To remove all database state and logs, delete the directory `%APPDATA%\ADC\`.

---

### Windows Troubleshooting

| Symptom | Cause | Solution |
| :--- | :--- | :--- |
| **SmartScreen blocks installer** | App reputation score on new release | Click "More info" → "Run anyway". |
| **Zero peer connections** | Windows Firewall blocking port 6881 | Open Windows Firewall settings and allow `nebulae.exe`. |
| **Crashes on startup** | Corrupted piece cache or SQLite lock | Open Settings → Show Advanced Diagnostics → Clear Internal Cache, or delete `%LOCALAPPDATA%\Nebulae\internal\cache\`. |

---

## 2. macOS Apple Silicon Installation

### Confirming Apple Silicon

1. Click the **Apple logo ()** in the top-left corner of your menu bar.
2. Select **About This Mac**.
3. In the summary window, find the line labeled **Chip**.
4. If it says **Apple M1, Apple M2, Apple M3, Apple M4, Apple M5** (or Pro / Max / Ultra), your Mac has Apple Silicon.

### Downloading Nebulae-macOS-arm64.dmg

1. Visit `https://github.com/koppot/Nebulae-torrent_client/releases`.
2. Download **`Nebulae-macOS-arm64.dmg`**.

---

### Step-by-Step macOS Installation

1. Double-click the downloaded **`Nebulae-macOS-arm64.dmg`** file.
2. A window opens showing **`Nebulae.app`** and a shortcut arrow pointing to your **Applications** folder.
3. Drag **`Nebulae.app`** into **Applications**.
4. In Finder, click the **Eject (⏏)** icon next to the mounted disk image.
5. Open **Applications** (or Launchpad) and double-click **Nebulae**.

---

### macOS Gatekeeper Verification

If using an unsigned or newly tagged community build, macOS Gatekeeper may display:
> *"Nebulae cannot be opened because Apple cannot check it for malicious software."*

#### Step-by-Step Bypass:
1. Open **Finder** → **Applications**.
2. **Right-click** (or hold `Control` and click) on **`Nebulae.app`**.
3. Select **Open** from the context menu.
4. A prompt appears asking: *"Are you sure you want to open it?"* Click **Open**.
5. *Alternative method*: Open **System Settings** → **Privacy & Security** → scroll down to the *Security* section → click **Open Anyway** next to the notice about Nebulae.

#### Quarantine Flag Fix (Terminal):
If macOS displays *"Nebulae is damaged and can't be opened"*, open Terminal and run:
```bash
xattr -cr /Applications/Nebulae.app
```
Press Enter. Nebulae will open immediately.

---

### First Launch & Permissions (macOS)

1. **Download Location Access**: When saving files to `~/Downloads` or an external drive, macOS will prompt: *"Nebulae would like to access files in your Downloads folder"*. Click **OK**.
2. **Speed Limits**: Open **Settings** to set upload and download limits to preserve your connection bandwidth.

---

### Adding Your First Torrent (macOS)

- Drag any `.torrent` file from Finder onto the Nebulae window.
- Click **+ Add Torrent** to paste an `adc://` or `magnet:?xt=...` link.

---

### Updating Nebulae on macOS

1. Download the new `Nebulae-macOS-arm64.dmg`.
2. Drag `Nebulae.app` into Applications and click **Replace**.
3. All preferences and torrent history remain safe in `~/Library/Application Support/ADC/`.

---

### Uninstalling Nebulae on macOS

1. Quit Nebulae (Command + Q).
2. Open Finder → **Applications** → Drag **`Nebulae.app`** to the **Trash**.
3. To delete all saved data, open Terminal and run:
   ```bash
   rm -rf ~/Library/Application\ Support/ADC
   ```

---

### macOS Troubleshooting

| Symptom | Cause | Solution |
| :--- | :--- | :--- |
| **"Nebulae is damaged" warning** | macOS Quarantine extended attribute | Run `xattr -cr /Applications/Nebulae.app` in Terminal. |
| **Cannot save to external drive** | Missing macOS Folder Permission | Open **System Settings** → **Privacy & Security** → **Files and Folders** → enable Nebulae for removable volumes. |
| **Where internal files and logs are stored** | Isolated internal directory | Stored at `~/Library/Application Support/Nebulae/internal/logs/nebulae_session.log`. |

---

## 3. macOS Intel Installation (x86_64)

### Confirming an Intel Mac

1. Click **Apple menu ()** → **About This Mac**.
2. Look at the line labeled **Processor**.
3. If it displays **Intel Core i5, Intel Core i7, Intel Core i9, or Intel Xeon**, your Mac uses an Intel CPU.

### Downloading Nebulae-macOS-x64.dmg

- Download **`Nebulae-macOS-x64.dmg`** from GitHub Releases.
- *Note*: Intel Macs cannot run the ARM64 build. Always verify you download `Nebulae-macOS-x64.dmg`.

### Installation & Gatekeeper (Intel)

1. Open `Nebulae-macOS-x64.dmg` and drag `Nebulae.app` into **Applications**.
2. If Gatekeeper prompts that the developer cannot be verified, right-click `Nebulae.app` → **Open** → **Open**.
3. If quarantine triggers, run:
   ```bash
   xattr -cr /Applications/Nebulae.app
   ```

---

## 4. Linux x86_64 Installation (.deb, .rpm, .AppImage)

Nebulae provides native Linux packages compiled against a pinned **Ubuntu 22.04 LTS (GLIBC 2.35)** baseline. This preserves broad binary compatibility across modern and enterprise distributions without requiring Docker or containerization.

For in-depth architectural notes, dependency trees, and display server troubleshooting, see the dedicated [Linux Packaging and Verification Guide](linux.md).

### Linux System Requirements

| Specification | Requirement |
| :--- | :--- |
| **Operating System** | Ubuntu 22.04+, Debian 12+, Linux Mint 21+, Fedora 38+, openSUSE Leap 15.6+ |
| **Architecture** | 64-bit x86 (`x86_64` / `amd64`) |
| **Desktop Environment** | GNOME, KDE Plasma, XFCE, Cinnamon, MATE, or Sway/wlroots (X11 or Wayland) |
| **Core Libraries** | WebKitGTK 4.1 or 4.0, GTK 3, Ayatana AppIndicator, OpenSSL |

---

### Debian, Ubuntu, and Linux Mint (`.deb`)

1. Download **`Nebulae-Linux-x86_64.deb`** from the latest GitHub Release.
2. Install via APT to automatically resolve dependencies:
   ```bash
   sudo apt update
   sudo apt install ./Nebulae-Linux-x86_64.deb
   ```
3. Alternatively, install via `dpkg`:
   ```bash
   sudo dpkg -i Nebulae-Linux-x86_64.deb
   sudo apt-get install -f -y
   ```
4. Launch Nebulae from your application menu or run `nebulae` in your terminal.

---

### Fedora and Red Hat (`.rpm`)

1. Download **`Nebulae-Linux-x86_64.rpm`** from GitHub Releases.
2. Install using DNF:
   ```bash
   sudo dnf install ./Nebulae-Linux-x86_64.rpm
   ```

---

### openSUSE (`.rpm`)

1. Download **`Nebulae-Linux-x86_64.rpm`** from GitHub Releases.
2. Install using Zypper:
   ```bash
   sudo zypper --non-interactive install --allow-unsigned-rpm ./Nebulae-Linux-x86_64.rpm
   ```

---

### Portable AppImage

1. Download **`Nebulae-Linux-x86_64.AppImage`**.
2. Mark it as executable:
   ```bash
   chmod +x Nebulae-Linux-x86_64.AppImage
   ```
3. Run the AppImage:
   ```bash
   ./Nebulae-Linux-x86_64.AppImage
   ```
*Note on FUSE*: If your distribution (e.g. minimal Ubuntu 24.04) does not have FUSE installed, run `./Nebulae-Linux-x86_64.AppImage --appimage-extract-and-run` or install `libfuse2t64` / `libfuse2`.

---

### Linux Desktop Integration

- **Application Launcher**: Registered automatically at `/usr/share/applications/nebulae.desktop`.
- **System Tray**: Displays status in the system notification tray via Ayatana AppIndicator.
- **Native File Choosers**: Seamlessly opens GTK file choosers (`zenity`) on GNOME/Cinnamon or Qt file choosers (`kdialog`) on KDE.
- **MIME Associations**: Automatically handles `.torrent` files, `.adc` files, and `magnet:` links.

---

### Uninstalling on Linux

- **Debian / Ubuntu**:
  ```bash
  sudo dpkg -r nebulae
  sudo dpkg --purge nebulae
  ```
- **Fedora / RHEL**:
  ```bash
  sudo dnf remove nebulae
  ```
- **openSUSE**:
  ```bash
  sudo zypper remove nebulae
  ```
- **Purge User Data & Logs**:
  ```bash
  rm -rf ~/.local/share/Nebulae ~/.config/ADC
  ```

---

## 5. Isolated Internal Storage & Diagnostics

To maintain a clean and reliable interface, Nebulae strictly isolates technical files:

```text
[macOS]   ~/Library/Application Support/Nebulae/internal/
[Windows] %LOCALAPPDATA%\Nebulae\internal\
          ├── db/             # SQLite metadata database (nebulae.db)
          ├── logs/           # Rotated session logs (nebulae_session.log)
          ├── cache/          # Fast resume state & piece cache
          ├── engine-state/   # BitTorrent engine checkpoints
          └── diagnostics/    # Crash and panic reports
```

### Accessing Diagnostics
1. Click the **Settings (gear)** icon in the top right.
2. Scroll to the bottom and click **Show Advanced Diagnostics**.
3. Available actions:
   - **Open Logs Folder**: Opens the isolated logs folder in Finder / Explorer.
   - **Copy Diagnostic Summary**: Copies sanitized system and engine state.
   - **Clear Internal Cache**: Safely purges temporary piece caches.
   - **Reset Settings to Factory Defaults**: Clears all user preferences.
   - **Export Logs (Sanitized)**: Displays logs with all magnet links and private URLs redacted.
