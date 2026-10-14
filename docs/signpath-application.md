# SignPath Foundation application — Nebulae (SUBMITTED 2026-10-14)

> **Status: submitted via the official form at <https://signpath.org/apply> on 2026-10-14 ("Form submitted - Thank you, we'll be in touch soon."). Awaiting SignPath Foundation response to admin@dimateploff.com. No acceptance exists yet, and no signing is enabled.**

## Application data (mapped to the official form fields)

| Form field | Value to submit |
| :--- | :--- |
| **Project Name** | `Nebulae` |
| **Repository URL** | `https://github.com/koppot/Nebulae-torrent_client` |
| **Homepage URL** | `https://github.com/koppot/Nebulae-torrent_client` |
| **Download URL** | `https://github.com/koppot/Nebulae-torrent_client/releases` (page will state that the project uses SignPath Foundation for code signing once accepted; see `docs/code-signing-policy.md`) |
| **Privacy Policy URL** | `https://github.com/koppot/Nebulae-torrent_client/blob/main/docs/privacy.md` |
| **Tagline** | `A native desktop torrent client for communities and archives` |
| **Category** | `Other` (file transfer / BitTorrent client) |
| **Description** | Nebulae is a native desktop BitTorrent client (Windows, macOS, Linux) built with Tauri and Rust. It combines a media-library style interface with torrent-management tooling, connectivity diagnostics, and optional per-application VPN binding for privacy-conscious communities and archives. It does not host, index, search, recommend, or distribute content. |
| **Reputation** | See the honest evidence summary below. |
| **Maintainer Type** | `Individual maintainer(s)` |
| **Build System** | `GitHub Actions` |
| **First Name / Last Name / Email** | Repository owner's real identity (owner supplies; not stored in this repository) |
| **Company Name** | — |
| **Primary Discovery Channel** | Owner's actual answer |

## Supporting detail

### Project purpose

Nebulae ("Torrent Client by Asian DVD Club") is a general-purpose peer-to-peer file-transfer client for communities, archives, and organizations sharing content with permission. It provides a dual UI (media shelves + power-user torrent manager), a connectivity diagnostics panel, optional per-application VPN binding, and community-tracker safeguards (automatic disable of DHT/PEX/LSD on private swarms, passkey masking, OS keychain storage).

### Requested signing scope

- `nebulae.exe` — the project-owned Windows executable built from this repository (Tauri 2 / Rust).
- `Nebulae-Setup-x64.exe` — the NSIS installer produced by the project's own build from that executable.

No third-party or upstream binaries will be submitted for signing. Bundled dependencies are permissively licensed open-source components; the Windows webview is the system-provided WebView2 runtime (system library).

### License summary

- Project license: **MIT** ([LICENSE](../LICENSE)), OSI-approved, no commercial dual-licensing.
- Rust dependencies (Tauri 2, tokio, serde, rusqlite bundled SQLite, reqwest, tracing, etc.): MIT / Apache-2.0 / BSD-style permissive licenses.
- Frontend dependencies (React, zustand, lucide, Tailwind, Vite, TypeScript): MIT / ISC.
- NSIS installer framework: zlib/libpng-style permissive license. SQLite (bundled): public domain.
- No proprietary components are bundled.

### Maintainer identity and role

- Maintainer and repository owner: GitHub user **[koppot](https://github.com/koppot)** (sole author of all 32 commits; account also maintains the project's release and CI infrastructure).
- Roles per the code-signing policy: koppot is the sole author/committer, reviewer of external pull requests, and signing approver.
- MFA: the owner must confirm/enable MFA on GitHub and later on SignPath (owner action; see final section).

### Maintenance and release history

- Repository created 2026-10-13; developed in the open with 32 commits by the maintainer (first commit 2026-10-13, latest 2026-10-14).
- Six public pre-releases published between 2026-10-13 and 2026-10-14 (`v0.1.0-beta.1` through `v0.1.0-beta.7`), each including the Windows NSIS installer `Nebulae-Setup-x64.exe` and standalone `nebulae.exe` with SHA-256 checksums.
- Automated multi-platform release pipeline (GitHub Actions) with post-build package verification (install/uninstall/launch tests on Linux; checksum publication for all platforms).

### Reputation evidence (honest)

SignPath requires verifiable reputation for executables. The truthful evidence available today is limited:

- **Public releases:** six tagged pre-releases with downloadable Windows/macOS/Linux artifacts and published checksums.
- **Commit history:** continuous, dated, single-maintainer development visible in the repository.
- **Community context:** the client is developed for the Asian DVD Club community (asiandvdclub.org); release announcements are posted to the project's community Discord channel.
- **Not available / not claimed:** GitHub stars or forks (currently zero), download statistics, media coverage, third-party testimonials, or an established long-term release history. The project is a few days old. **This is the main acceptance risk, and the application does not overstate it.**

### Build provenance explanation

All release binaries are produced by [`.github/workflows/release.yml`](../.github/workflows/release.yml) on **GitHub-hosted runners** from a tagged commit. The Windows job builds the frontend (TypeScript/Vite) and the Rust executable with the Tauri 2 CLI, bundles the NSIS installer, publishes SHA-256 checksums, and uploads artifacts as GitHub workflow artifacts. The prepared SignPath integration (`docs/signpath-ci-integration.md`) submits `nebulae.exe` and then the installer via `signpath/github-action-submit-signing-request@v3`, with the SignPath GitHub trusted-build integration verifying origin. Every signing request will require manual approval in the SignPath console.

### Connectivity and VPN functionality (accurate, against the actual implementation)

**Connectivity Doctor (current implementation):** reports connectivity diagnostics for the user's own torrenting setup. In the current beta build, the report values are **simulated placeholders** (`src-tauri/src/commands/connectivity.rs`); the only real probe implemented so far is a local socket bind test of the application's own listening port (`retest_listening_port`), which checks whether the port is available locally. It does **not** scan other systems, does not probe third-party hosts for vulnerabilities, and does not circumvent any security control. Planned real probes (documented in `docs/connectivity.md`) are limited to: reachability of the application's own listening port, availability of UPnP/NAT-PMP/PCP on the user's own router, NAT type classification, IPv4/IPv6 connectivity of the user's own machine, DNS latency, and reachability of the tracker the user configured — standard, consent-based diagnostics of the same kind shipping in mainstream torrent clients.

**UPnP / system changes:** the settings model contains an `enable_upnp` option (default on, user-visible and user-controllable). No UPnP mapping code is implemented yet. When implemented, port mapping will create a router mapping with user-visible status and a settings control to disable it, satisfying the "announce system changes" condition; no other system configuration is modified.

**VPN functionality (current implementation):** the settings and UI for VPN modes exist (No VPN / System VPN / Application-only), and WireGuard `.conf` profile parsing is implemented (OpenVPN import is a stub). Actual socket binding to a VPN adapter, tunnel management, and the kill switch are **not yet implemented**; displayed VPN status values are simulated placeholders (`src-tauri/src/commands/vpn.rs`, `engine/mock.rs`). Nebulae does not create VPN tunnels, does not bypass network security controls, and only ever binds the application's own torrent sockets to a VPN interface the user configured. Other applications are never routed or affected.

No feature in the codebase scans systems for exploitable vulnerabilities; the SignPath Foundation "no hacking tools" condition is met by design.

### Privacy

`docs/privacy.md`: local-first, zero telemetry; data leaves the machine only when the user adds a tracker/torrent/poster fetch. Signed packages contain no secrets or runtime data.

### Automatic Updates and User Consent (SignPath condition compliance)

Nebulae implements silent background auto-updates compliant with SignPath Foundation conditions:
- **Installer Notification**: The Windows NSIS installer displays a visible disclosure during setup notifying users that signed updates are applied automatically in the background.
- **User Opt-Out**: Users can disable automatic updates at any time with a single toggle in **Settings → Options (Behavior)**.
- **Zero Telemetry When Disabled**: Turning off the toggle performs an immediate early-return before making any network socket connection or DNS request.
- **Staged Application**: Updates download quietly without popups or transfer interruptions, and stage to apply strictly upon natural application quit.
- **Minisign Verification**: Update payloads are verified against the hardcoded Minisign public key before execution.

### Honest beta status and unresolved questions

- The application state is **early beta**: the BitTorrent engine currently runs a simulated mock (`src-tauri/src/engine/mock.rs`); transfer, connectivity, and VPN status values are placeholders. The UI, persistence, torrent metadata parsing, and packaging are real and functional.
- Unresolved questions for SignPath (to raise during review, not conceal):
  1. Acceptance criteria given the project's very young public history (no stars/forks/downloads yet); the project will gather real usage before or during review.
  2. Confirmation that the described connectivity diagnostics (own-port reachability, own-router UPnP mapping, configured-tracker checks) fall outside the "no hacking tools" clause.
  3. Handling of the pre-signing rename of the NSIS installer with respect to metadata restrictions.
  4. Whether the simulated engine status is acceptable for initial releases, or whether the Foundation prefers signing to start once the real engine ships.

## Concise application message (for the official form/contact)

> Nebulae (<https://github.com/koppot/Nebulae-torrent_client>) is a free, open-source (MIT) native desktop BitTorrent client for communities and archives, built with Tauri 2 and Rust by a single maintainer (koppot). We request free code signing for our two project-owned Windows artifacts: the `nebulae.exe` executable and our NSIS installer `Nebulae-Setup-x64.exe`, both built exclusively by GitHub Actions from tagged commits in the public repository, on GitHub-hosted runners, with per-release manual approval and post-signing SHA-256 checksums. The project is an early beta: six public pre-releases with Windows/macOS/Linux artifacts are published, but the project is new (repository created 2026-10-13) and has no stars or download statistics yet — we are stating this plainly rather than inflating reputation. Our code-signing policy (docs/code-signing-policy.md) lists roles (sole maintainer koppot as author, reviewer, and approver), our privacy policy (docs/privacy.md), and the required SignPath attribution. The app includes a Connectivity Doctor that checks the client's own listening port, router port-mapping availability, and configured-tracker reachability (currently partly simulated diagnostics; no scanning of other systems), and a planned application-scoped VPN binding mode that only ever binds our own sockets to a user-configured VPN adapter — no security circumvention, no system changes without consent (UPnP mapping will be user-controlled). We will use MFA on GitHub and SignPath and will keep unsigned beta builds labeled as such until signing is enabled.
