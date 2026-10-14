# Nebulae — Project Roadmap

This document outlines the phased development plan for **Nebulae** (Torrent Client by Asian DVD Club).

---

## Phase 1: Architecture, Scaffolding & Design (Completed)
- [x] Select and evaluate BitTorrent engine architecture and adapter interfaces.
- [x] Configure desktop naming conventions: product as `Nebulae`, executable as `nebulae` / `nebulae.exe`, subtitle as `Torrent Client by Asian DVD Club`.
- [x] Scaffold Tauri 2 + Rust + React + Vite + TypeScript + Tailwind CSS structure.
- [x] Implement cinematic multicolor splash screen with community invitation (`asiandvdclub.org`).
- [x] Implement UI mode switcher (Simple Media Mode vs Torrent Manager Mode vs Compact Bar).
- [x] Implement in-app beginner-friendly Installation & User Guide with platform tabs.
- [x] Implement user-friendly Settings with hidden Advanced Diagnostics panel (*Der Sperre*).
- [x] Implement isolated internal directory architecture (`Nebulae/internal/{db, logs, cache, engine-state, diagnostics}`).
- [x] Establish sanitized logging with privacy safeguards (no magnet links, passkeys, or private keys in logs).
- [x] Create comprehensive `docs/installation.md` for Windows x64, macOS Apple Silicon, and macOS Intel.

---

## Phase 2: Engine Integration & Core BitTorrent Operations
- [x] Connect `librqbit::Session` directly into Tauri backend state (`RqbitTorrentEngine`).
- [x] Enforce Three-Tier Honesty Policy: MockTorrentEngine disallowed in release; FailedTorrentEngine & RecoveryScreen.
- [x] Implement `.torrent` file parsing, hashing, and storage allocation (metainfo).
- [x] Wire up real-time download and upload speeds in Simple Media and Manager views.
- [x] Implement torrent lifecycle controls: Pause, Resume, Remove, Delete Files, Force Recheck.
- [x] Implement real `.torrent` file export serialization (`export_torrent_file`).
- [x] Wire live peer telemetry from engine into Smart Peering algorithm (`get_peer_telemetry`).
- [x] Real parallel Connectivity Doctor probes (UPnP SSDP, NAT-PMP, IPv4/IPv6, DNS latency, ADC tracker).
- [x] Honest ADC Tracker profile statistics & Seed Time Shield background loop (`spawn_seeding_rules_loop`).
- [x] Real VPN subsystem: OS Keychain credential storage via `keyring`, OpenVPN `.ovpn` parser, system interface discovery, and 500ms fail-closed kill-switch watcher loop (`spawn_kill_switch_watcher`).
- [x] Implement throttled 500ms telemetry IPC stream from Rust to React frontend (`spawn_telemetry_loop`).
- [x] Implement magnet link resolver (`ut_metadata` BEP 9).
- [x] Implement file priority tree (allow selective file downloading and skipping).

---

## Phase 3: Advanced Torrent Management (Power-User Parity)
- [x] Per-torrent upload and download speed limit overrides.
- [x] Sequential downloading toggle for active media torrents.
- [x] Detailed Inspector tabs:
  - **Trackers**: Announce URLs, tier hierarchy, seed/peer feedback, re-announce trigger.
  - **Peers**: Peer IPs, client ID, encryption flags, transfer rates.
  - **Files**: Interactive file checklist with priority adjustment (Do Not Download / Normal / High).
  - **Speed Graph**: Live SVG transfer throughput charts.
- [x] Watched Folders subsystem (auto-load dropped `.torrent` files via `spawn_watched_folder_loop`).
- [x] RSS feed automation engine (regex title matching, auto-queueing via `spawn_rss_loop`).
- [x] Seeding rules: auto-pause upon reaching target share ratio or seed time (`spawn_seeding_rules_loop`).

---

## Phase 4: Local Media Library & Playback Station
- [x] Local media metadata extraction (parse video release titles without cloud mandates).
- [x] Embedded video frame thumbnail generator & optical disc artwork scanner (`get_media_thumbnail`, `scan_disc_artwork`).
- [x] Local streaming launcher: stream/play media in default system players (VLC, IINA, mpv) with hidden command execution (`launch_media_player`, `launch_disc_player`).
- [x] Automatic series, episode & multi-disc boxset aggregator (`SimpleMediaView` boxset carousels).
- [x] Disk space cleaner and completed media organizer (`clean_cache_files`, `organize_completed_media`, `get_volume_disk_space`).
