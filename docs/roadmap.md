# Nebula Torrent (ADC) — Project Roadmap

This document outlines the phased development plan for **ADC** (Nebula Torrent).

---

## Phase 1: Architecture, Scaffolding & Design (Completed)
- [x] Select and evaluate BitTorrent engine (`librqbit`).
- [x] Configure desktop naming conventions: executable/package as `ADC`, brand as `Nebula Torrent`.
- [x] Scaffold Tauri 2 + Rust + React + Vite + TypeScript + Tailwind CSS structure.
- [x] Implement cinematic multicolor splash screen with community invitation (`asiandvdclub.org`).
- [x] Implement UI mode switcher (Simple Media Mode vs Torrent Manager Mode).
- [x] Implement in-app beginner-friendly Installation & User Guide with platform tabs.
- [x] Implement user-friendly Settings with hidden Advanced Diagnostics panel.
- [x] Implement isolated internal directory architecture (`ADC/internal/{db, logs, cache, engine, diagnostics}`).
- [x] Establish sanitized logging with privacy safeguards (no magnet links or private keys in logs).
- [x] Create comprehensive `docs/installation.md` for Windows x64, macOS Apple Silicon, and macOS Intel.

---

## Phase 2: Engine Integration & Core BitTorrent Operations
- [ ] Connect `librqbit::Session` directly into Tauri backend state.
- [ ] Implement magnet link resolver (`ut_metadata` BEP 9).
- [ ] Implement `.torrent` file parsing, hashing, and storage allocation.
- [ ] Implement file priority tree (allow selective file downloading and skipping).
- [ ] Implement throttled 500ms telemetry IPC stream from Rust to React frontend.
- [ ] Wire up real-time download and upload speeds in Simple Media and Manager views.
- [ ] Implement torrent lifecycle controls: Pause, Resume, Remove, Delete Files, Force Recheck.

---

## Phase 3: Advanced Torrent Management (ruTorrent Parity)
- [ ] Per-torrent upload and download speed limit overrides.
- [ ] Sequential downloading toggle for active media torrents.
- [ ] Detailed Inspector tabs:
  - **Trackers**: Announce URLs, tier hierarchy, seed/peer feedback, re-announce trigger.
  - **Peers**: Peer IPs, client ID, encryption flags, transfer rates.
  - **Files**: Interactive file checklist with priority adjustment (Do Not Download / Normal / High).
  - **Speed Graph**: Live SVG/Canvas transfer throughput charts.
- [ ] Watched Folders subsystem (auto-load dropped `.torrent` files).
- [ ] RSS feed automation engine (regex title matching, auto-queueing).
- [ ] Seeding rules: auto-pause upon reaching target share ratio or seed time.

---

## Phase 4: Local Media Library & Playback Station
- [ ] Local media metadata extraction (parse video release titles without cloud mandates).
- [ ] Embedded video frame thumbnail generator.
- [ ] Local streaming launcher: stream in-progress sequential downloads to default system players (VLC, IINA, mpv).
- [ ] Automatic series and episode aggregator.
- [ ] Disk space cleaner and completed media organizer.
