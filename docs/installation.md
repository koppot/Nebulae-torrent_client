# ADC Installation and User Guide

This guide provides step-by-step, beginner-friendly instructions for installing, configuring, and updating **ADC** (executable name for **Nebula Torrent**) on Windows and macOS.

---

## Table of Contents

1. [Understanding ADC and Nebula Torrent](#understanding-adc-and-nebula-torrent)
2. [Windows x64 Installation (NSIS Installer)](#1-windows-x64-installation-nsis-installer)
   - [System Requirements](#windows-system-requirements)
   - [Downloading ADC-Setup-x64.exe](#downloading-adc-setup-x64exe)
   - [Windows SmartScreen & Antivirus Notice](#windows-smartscreen--antivirus-notice)
   - [Step-by-Step Installation Walkthrough](#step-by-step-installation-walkthrough)
   - [First Launch & Windows Firewall Configuration](#first-launch--windows-firewall-configuration)
   - [Adding Your First Torrent](#adding-your-first-torrent-windows)
   - [Updating ADC on Windows](#updating-adc-on-windows)
   - [Uninstalling ADC on Windows](#uninstalling-adc-on-windows)
   - [Windows Troubleshooting & Log Locations](#windows-troubleshooting)
3. [macOS Apple Silicon Installation (M1, M2, M3, M4)](#2-macos-apple-silicon-installation)
   - [Confirming Apple Silicon](#confirming-apple-silicon)
   - [Downloading ADC-macOS-arm64.dmg](#downloading-adc-macos-arm64dmg)
   - [Step-by-Step Installation Walkthrough](#step-by-step-macos-installation)
   - [macOS Gatekeeper Verification](#macos-gatekeeper-verification)
   - [First Launch & Permissions](#first-launch--permissions-macos)
   - [Adding Your First Torrent](#adding-your-first-torrent-macos)
   - [Updating ADC on macOS](#updating-adc-on-macos)
   - [Uninstalling ADC on macOS](#uninstalling-adc-on-macos)
   - [macOS Troubleshooting & Log Locations](#macos-troubleshooting)
4. [macOS Intel Installation (x86_64)](#3-macos-intel-installation-x86_64)
   - [Confirming an Intel Mac](#confirming-an-intel-mac)
   - [Downloading ADC-macOS-x64.dmg](#downloading-adc-macos-x64dmg)
   - [Installation & Gatekeeper Walkthrough](#installation--gatekeeper-intel)
5. [Isolated Internal Storage & Diagnostics](#isolated-internal-storage--diagnostics)

---

## Understanding Nebula and ADC

- **Application Name**: **Nebula**
- **Subtitle**: **Torrent Client by Asian DVD Club**
- **Shipped Executable & Package Name**: **nebular_torrent** (e.g., `nebular_torrent.exe`, `nebular_torrent.app`, `nebular_torrent-Setup-x64.exe`, `nebular_torrent-macOS-arm64.dmg`, `nebular_torrent-macOS-x64.dmg`).
- **Startup Splash Screen**: On launch, Nebula presents a cinematic startup splash screen with internal initialization progress and a community invitation to visit `asiandvdclub.org`. The splash screen can be skipped immediately or disabled in Settings.
- **Hidden Internal Files**: Database files, session logs, engine state, and caches are isolated in an internal directory (`ADC/internal/`) and never exposed in the standard interface.

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

### Downloading nebular_torrent-Setup-x64.exe

1. Navigate to the official releases repository: `https://github.com/nebula-torrent/nebula-torrent/releases`
2. Under the latest release header, locate the **Assets** section.
3. Download **`nebular_torrent-Setup-x64.exe`**.

### Windows SmartScreen & Antivirus Notice

When running newly released open-source software on Windows, **Windows Defender SmartScreen** or third-party antivirus utilities may display a warning dialog stating:
> *"Windows protected your PC — Microsoft Defender SmartScreen prevented an unrecognized app from starting."*

#### Why this happens:
Open-source software distributed without purchasing expensive corporate Extended Validation (EV) code-signing certificates triggers SmartScreen until thousands of global users establish reputation with Microsoft's servers.

#### How to safely proceed:
1. Verify the download originated from the official GitHub Releases page.
2. Click **More info** (underlined text in the dialog).
3. Click the **Run anyway** button that appears.

---

### Step-by-Step Installation Walkthrough

1. Double-click **`ADC-Setup-x64.exe`** in your Downloads folder.
2. If prompted by **User Account Control (UAC)**, click **Yes** to allow the installer to make changes.
3. **Choose Install Location**: The installer installs to your user profile (`%LOCALAPPDATA%\Programs\ADC`) by default, requiring no administrative privileges. Click **Next**.
4. **Choose Shortcuts**: Ensure *Create Desktop Shortcut* and *Create Start Menu Shortcut* are selected. Click **Install**.
5. **Finish**: Check **Launch ADC** and click **Finish**.

---

### First Launch & Windows Firewall Configuration

When ADC launches for the first time, Windows Defender Firewall will prompt:
> *"Windows Defender Firewall has blocked some features of ADC on all public and private networks."*

#### Why Firewall permission is required:
BitTorrent is a decentralized peer-to-peer network. To receive piece data and discover peers, ADC must accept inbound and outbound TCP/UDP connections on its designated listening port (default: `6881`).

#### What to select:
1. Check **Private networks, such as my home or work network**.
2. Uncheck Public networks (unless you are on public Wi-Fi).
3. Click **Allow access**.

---

### Adding Your First Torrent (Windows)

You can add torrents using three intuitive methods:
1. **Magnet Links**: Click the pink **+ Add Torrent** button at the top right, paste your magnet link (`magnet:?xt=urn:btih:...` or `adc://...`), and click **Start Download**.
2. **File Drag-and-Drop**: Drag any `.torrent` file from Windows Explorer directly onto the ADC window. The screen will highlight with a pink drop border.
3. **File Association**: Double-click any `.torrent` file on your computer; Windows will automatically launch ADC and begin the transfer.

---

### Updating ADC on Windows

1. Download the new `ADC-Setup-x64.exe` from GitHub Releases.
2. Run the installer. It will automatically detect your existing installation, upgrade the executable, and preserve all settings, torrent metadata, and downloaded files.

---

### Uninstalling ADC on Windows

1. Open **Windows Settings** (Windows Key + I).
2. Go to **Apps** → **Installed apps** (or *Apps & features*).
3. Scroll down or search for **ADC**.
4. Click the three dots (`...`) and select **Uninstall**.
5. Follow the uninstallation wizard. (Downloaded media files remain in your Downloads folder).

---

### Windows Troubleshooting

| Symptom | Cause | Solution |
| :--- | :--- | :--- |
| **App does not open / missing DLL** | Missing VC++ Runtime | Install the [Visual C++ 2015–2022 Redistributable (x64)](https://aka.ms/vs/17/release/vc_redist.x64.exe) from Microsoft. |
| **Downloads stuck at 0% / "Finding peers"** | Firewall or router blocking port | Check Windows Firewall rules for ADC; verify UPnP is enabled in **Settings → Network**. |
| **Antivirus false positive** | Heuristic signature detection | Add `%LOCALAPPDATA%\Programs\ADC\ADC.exe` to your antivirus exclusion list. |
| **Where logs and settings are stored** | Internal isolated directory | Open `%APPDATA%\ADC\internal\logs\` to inspect `adc_session.log`. |

---

## 2. macOS Apple Silicon Installation

### Confirming Apple Silicon

1. Click the **Apple logo ()** in the top-left corner of your menu bar.
2. Select **About This Mac**.
3. In the summary window, find the line labeled **Chip**.
4. If it says **Apple M1, Apple M2, Apple M3, Apple M4** (or Pro / Max / Ultra), your Mac has Apple Silicon.

### Downloading nebular_torrent-macOS-arm64.dmg

1. Visit `https://github.com/nebula-torrent/nebula-torrent/releases`.
2. Download **`nebular_torrent-macOS-arm64.dmg`**.

---

### Step-by-Step macOS Installation

1. Double-click the downloaded **`nebular_torrent-macOS-arm64.dmg`** file.
2. A window opens showing **nebular_torrent.app** and a shortcut arrow pointing to your **Applications** folder.
3. Drag **`nebular_torrent.app`** into **Applications**.
4. In Finder, click the **Eject (⏏)** icon next to the mounted disk image.
5. Open **Applications** (or Launchpad) and double-click **nebular_torrent**.

---

### macOS Gatekeeper Verification

If using an unsigned or newly tagged community build, macOS Gatekeeper may display:
> *"ADC cannot be opened because Apple cannot check it for malicious software."*

#### Step-by-Step Bypass:
1. Open **Finder** → **Applications**.
2. **Right-click** (or hold `Control` and click) on **`ADC.app`**.
3. Select **Open** from the context menu.
4. A prompt appears asking: *"Are you sure you want to open it?"* Click **Open**.
5. *Alternative method*: Open **System Settings** → **Privacy & Security** → scroll down to the *Security* section → click **Open Anyway** next to the notice about ADC.

#### Quarantine Flag Fix (Terminal):
If macOS displays *"ADC is damaged and can't be opened"*, open the Terminal app and paste:
```bash
xattr -cr /Applications/ADC.app
```
Press Enter. ADC will open immediately.

---

### First Launch & Permissions (macOS)

1. **Download Location Access**: When saving files to `~/Downloads` or an external drive, macOS will prompt: *"ADC would like to access files in your Downloads folder"*. Click **OK**.
2. **Speed Limits**: Open **Settings** to set upload and download limits to preserve your connection bandwidth.

---

### Adding Your First Torrent (macOS)

- Drag any `.torrent` file from Finder onto the ADC window.
- Click **+ Add Torrent** to paste an `adc://` or `magnet:?xt=...` link.

---

### Updating ADC on macOS

1. Download the new `ADC-macOS-arm64.dmg`.
2. Drag `ADC.app` into Applications and click **Replace**.
3. All preferences and torrent history remain safe in `~/Library/Application Support/ADC/`.

---

### Uninstalling ADC on macOS

1. Quit ADC (Command + Q).
2. Open Finder → **Applications** → Drag **`ADC.app`** to the **Trash**.
3. To delete all saved data, open Terminal and run:
   ```bash
   rm -rf ~/Library/Application\ Support/ADC
   ```

---

### macOS Troubleshooting

| Symptom | Cause | Solution |
| :--- | :--- | :--- |
| **"ADC is damaged" warning** | macOS Quarantine extended attribute | Run `xattr -cr /Applications/ADC.app` in Terminal. |
| **Cannot save to external drive** | Missing macOS Folder Permission | Open **System Settings** → **Privacy & Security** → **Files and Folders** → enable ADC for removable volumes. |
| **Where internal files and logs are stored** | Isolated internal directory | Stored at `~/Library/Application Support/ADC/internal/logs/adc_session.log`. |

---

## 3. macOS Intel Installation (x86_64)

### Confirming an Intel Mac

1. Click **Apple menu ()** → **About This Mac**.
2. Look at the line labeled **Processor**.
3. If it displays **Intel Core i5, Intel Core i7, Intel Core i9, or Intel Xeon**, your Mac uses an Intel CPU.

### Downloading nebular_torrent-macOS-x64.dmg

- Download **`nebular_torrent-macOS-x64.dmg`** from GitHub Releases.
- *Note*: Intel Macs cannot run the ARM64 build. Always verify you download `nebular_torrent-macOS-x64.dmg`.

### Installation & Gatekeeper (Intel)

1. Open `nebular_torrent-macOS-x64.dmg` and drag `nebular_torrent.app` into **Applications**.
2. If Gatekeeper prompts that the developer cannot be verified, right-click `nebular_torrent.app` → **Open** → **Open**.
3. If quarantine triggers, run:
   ```bash
   xattr -cr /Applications/nebular_torrent.app
   ```

---

## Isolated Internal Storage & Diagnostics

To maintain a clean and reliable interface, ADC strictly isolates technical files:

```text
[macOS]   ~/Library/Application Support/ADC/internal/
[Windows] %APPDATA%\ADC\internal\
          ├── db/             # SQLite metadata database (nebula.db)
          ├── logs/           # Rotated session logs (adc_session.log)
          ├── cache/          # Fast resume state & piece cache
          ├── engine/         # BitTorrent engine checkpoints
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
