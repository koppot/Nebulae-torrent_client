# Contributing to Nebulae

Thank you for your interest in contributing to **Nebulae** (Torrent Client by Asian DVD Club). Nebulae is built to deliver exceptional performance, high-scale seeding stability, and robust cross-platform networking on macOS and Windows.

To maintain code quality, reliability, and security across the codebase, please review the following guidelines before submitting issues or pull requests.

---

## Architectural Principles

1. **Safety and Memory Discipline**: The backend is implemented in Rust (`src-tauri`), maintaining zero raw pointer risks, strict resource management, and minimal resident set size (RSS) under workloads scaling from dozens to tens of thousands of torrents.
2. **Clear Separation of UI and Engine**: The frontend (React, TypeScript, Tailwind CSS) presents real-time telemetry and control surfaces. It communicates with the backend exclusively via typed Tauri IPC invocations and events.
3. **Observational vs. Operational Boundaries**: We do not simulate or present mock behaviors as functional production capabilities. If an engine or operating system API does not expose a hook (e.g. per-peer request queue scheduling in `librqbit`), the feature is explicitly designated as observational until real adapters are implemented.
4. **Privacy-Preserving Diagnostics**: Secret management (passkeys, VPN certificates, private keys) belongs strictly in platform keychains (macOS Keychain, Windows Credential Manager). Logging and diagnostics must redact all sensitive identifiers before export.

---

## Development Environment Setup

### Prerequisites
- **Node.js**: v20+ with **pnpm** (v9+)
- **Rust**: Stable toolchain (1.80+ recommended) with `clippy` and `rustfmt`
- **Platform Dependencies**:
  - **macOS**: Xcode Command Line Tools (`xcode-select --install`)
  - **Windows**: Visual Studio 2022 C++ Build Tools, WebView2 Runtime
  - **Linux** (for CI/container verification): `libwebkit2gtk-4.1-dev`, `build-essential`, `libssl-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`

### Installation
Clone the repository and install frontend dependencies:

```bash
git clone https://github.com/koppot/Nebulae-torrent_client.git
cd Nebulae-torrent_client
pnpm install
```

### Running in Development
Start the Tauri desktop application with hot module replacement:

```bash
pnpm tauri dev
```

To run only the frontend in a standard browser for component work:

```bash
pnpm run dev
```

---

## Code Quality Standards

Before submitting any Pull Request, ensure all checks pass locally:

### 1. Frontend Checks
```bash
# Type check and build bundle
pnpm run build
```

### 2. Rust Backend Checks
```bash
cd src-tauri

# Format check
cargo fmt --all -- --check

# Strict linting
cargo clippy --all-targets --all-features -- -D warnings

# Unit and integration tests
cargo test --all-targets --all-features
```

---

## Branching and Commit Guidelines

- Work on descriptive feature or fix branches branched off `main`:
  `git checkout -b feature/vpn-interface-binding` or `git checkout -b fix/peer-telemetry-parsing`
- Keep commit messages concise, imperative, and structured:
  - `feat(vpn): implement platform network interface enumeration`
  - `fix(doctor): correct UPnP timeout handling in NAT discovery`
  - `docs: update build instructions for Apple Silicon`
- Never commit build output (`dist/`, `target/`), SQLite files (`*.db`), log files, private VPN configurations, or sensitive credentials.

---

## Pull Request Process

1. Open a Pull Request referencing relevant issues using the provided PR template.
2. Provide a clear summary of modifications and how they were tested.
3. Ensure CI runs cleanly across all jobs.
4. Maintain documentation integrity: if you change configuration flags or IPC contracts, update the corresponding documentation in `docs/`.

---

## Community

Community discussions and community-supported workflows take place at [Asian DVD Club](https://asiandvdclub.org). Consider joining our community to participate in testing, tracker discussions, and feature feedback.
