# Security Policy

The Nebulae project takes application security, network privacy, and credential safety seriously. Because BitTorrent clients manage raw network sockets, VPN tunnels, and tracker credentials, we enforce strict security and isolation boundaries.

---

## Supported Versions

Only the latest release and the current active development branch (`main`) receive security updates.

| Version | Supported |
| :--- | :--- |
| `0.1.x` (alpha) | :white_check_mark: |
| `< 0.1.0` | :x: |

---

## Reporting a Vulnerability

If you discover a security vulnerability in Nebulae, please **do not open a public GitHub issue**.

To report a vulnerability responsibly:

1. Send an email describing the vulnerability, affected components, steps to reproduce, and potential impact to the repository maintainers or via GitHub's private vulnerability reporting feature:
   `https://github.com/koppot/Nebulae-torrent_client/security/advisories/new`
2. Include the following details if applicable:
   - Operating system and version
   - Application version or commit hash
   - Proof-of-concept code, exploit scenario, or packet trace
   - Disclosure preferences or coordination timeline

You will receive an initial response within 48 hours. We will collaborate with you to validate the issue, prepare an advisory, and schedule a patch release.

---

## Security Architecture & Design Principles

Nebulae implements defense-in-depth principles across all platform layers:

### 1. Isolated Application Storage
All application databases, configuration files, and logs reside strictly within an isolated application directory:
- **macOS**: `~/Library/Application Support/Nebulae/internal/`
- **Windows**: `%LOCALAPPDATA%\Nebulae\internal\`
- **Linux**: `~/.local/share/Nebulae/internal/`

Runtime state is never mixed with user document folders or exposed publicly.

### 2. Secret Redaction & Keychain Storage
- **Passkeys & Private Keys**: BitTorrent passkeys and VPN private keys are never stored in plain-text SQLite databases or application logs.
- **Platform Keychains**: Operating-system-backed secure keychains (macOS Keychain, Windows Credential Manager) are the authoritative destination for sensitive credentials.
- **Log Sanitization**: The diagnostic engine automatically sanitizes IP addresses, hostnames, and token strings before log export.

### 3. VPN Kill Switch & Network Binding
- Traffic bound to a designated network interface (e.g., VPN tunnel) must fail closed if the interface terminates or loses default gateway routing.
- The client prevents traffic leakage over secondary default physical routes.

### 4. Memory Safety & Rust Tooling
- The core networking, parsing, and persistence backend is implemented in 100% safe Rust.
- Continuous integration tests enforce strict compiler linter rules (`-D warnings`) and automated dependency auditing.
