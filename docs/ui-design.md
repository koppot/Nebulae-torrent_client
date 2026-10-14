# Nebulae — UI Design Specification

## 1. Visual Identity & Brand Philosophy

- **Application Brand**: Nebulae
- **Product Subtitle**: Torrent Client by Asian DVD Club
- **Executable Identity**: `nebulae` / `nebulae.exe`
- **Application Bundle**: `Nebulae.app`
- **Public Application Icon**: Original Nebulae App Icon (`src/assets/app-icon.png`)
- **Developer Icon**: *Der Sperre* (`src/assets/dev-icon.png`), used exclusively in the Advanced Diagnostics and internal developer subsystem.
- **Design Inspiration**: Premium Asian-cinema archive aesthetic blended with modern, clean desktop minimalism.
- **Tone**: Cinematic, focused, high contrast, non-intrusive.

---

## 2. Color System Tokens

| Token | Hex | Role | Usage |
| :--- | :--- | :--- | :--- |
| `adc-dark` | `#0B0F19` | Primary Background | Window background in dark theme |
| `adc-cardDark` | `#131B2E` | Card Background | Surface color for poster cards and panels |
| `adc-cardBorderDark` | `#1E293B` | Structural Borders | Hairline borders between shelves and inspector |
| `adc-accent` | `#EC4899` | Cinematic Magenta | Primary brand highlight, active tabs, progress |
| `adc-gold` | `#F59E0B` | Archive Goldenrod | Subtitles, "Ready to Watch" controls, warnings |
| `adc-cyan` | `#06B6D4` | Torrent Cyan | Download speeds, active transfer indicators |
| `adc-emerald` | `#10B981` | Seeding Green | Upload speeds, ratio success states, completed |
| `adc-violet` | `#8B5CF6` | Deep Violet | Ambient lighting in cinematic splash screen |

---

## 3. Typography Scale

- **Sans Serif**: Inter / -apple-system / BlinkMacSystemFont
  - Used for titles, section headers, buttons, and navigation.
- **Monospace**: JetBrains Mono / SFMono-Regular / Menlo
  - Used for transfer rates, byte counts, ETAs, share ratios, and hashes.

---

## 4. UI Modes

### Mode 1: Simple Media Mode
- Horizontal shelves:
  1. *Active Downloads*: Real-time progress bars, speed badges, ETA.
  2. *Ready to Watch*: Media posters, quick play button, folder jump.
  3. *Completed & Seeding*: Ratio indicator, file size.
- Full-window drag-and-drop overlay for dropping `.torrent` files.

### Mode 2: Advanced Torrent Manager Mode
- Comprehensive transfer management with modern polish:
  - Sidebar with Status Filters and Categories.
  - Action Toolbar for batch operations.
  - Virtualized data grid displaying all transfer metrics.
  - Bottom Tabbed Inspector: General, Files, Trackers, Peers, Speed.

---

## 5. Startup Splash Screen
- Anamorphic multicolor ambient lighting.
- Product Title: **Nebulae**.
- Subtitle: **Torrent Client by Asian DVD Club**.
- Community Callout:
  ```text
  Consider joining our community
  asiandvdclub.org
  ```
- Subsystem initialization progress bar.
- Skip button.
- User-configurable in Settings.
