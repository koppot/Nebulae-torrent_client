# AsianDVDClub Tracker Integration Framework

## 1. Overview & Community Alignment

Nebulae provides a community tracker framework designed for compatibility with **AsianDVDClub** (`asiandvdclub.org`), supporting archival preservation and lawful community sharing.

Nebulae interacts strictly through standard BitTorrent protocol mechanics and documented tracker capabilities. Nebulae **never scrapes web pages**, calls undocumented private endpoints, or bypasses tracker authentication rules. Live account linking remains in a local preview state until an official tracker specification is published and approved.

---

## 2. Passkey Security & Storage

- **OS Keychain Integration**: When personal credentials are linked, passkeys are stored in the operating system's native secure credential manager:
  - **macOS**: Apple Keychain Services (`Security.framework`)
  - **Windows**: Windows Credential Manager (`wincred`)
- **Masking in UI**: Passkeys are masked throughout the application interface (e.g. `adc_pk_••••••••••••••••7890`).
- **Zero Logging Guarantee**: The internal `SafeLogger` automatically redacts passkeys, authentication parameters, and announce query strings from all session logs, diagnostics, and export packages.

---

## 3. Private Swarm Safeguards (BEP 27)

When a `.torrent` file contains the private flag (`info.private = 1`):

1. **DHT is permanently disabled** for that swarm.
2. **Peer Exchange (PEX) is permanently disabled** for that swarm.
3. **Local Service Discovery (LSD) is permanently disabled** for that swarm.
4. **Tracker Exclusivity**: Peer discovery is conducted strictly through the authenticated announce URL.

These safeguards ensure strict compliance with private tracker rules and prevent passkey leakage across public swarms.

---

## 4. Local Tools & Ratio Protection

### Local Ratio Coach
- Proactive ratio impact projection:
  $$\text{Ratio}_{\text{projected}} = \frac{\text{Uploaded}}{\text{Downloaded} + \Delta\text{Downloaded}}$$
- Helps users verify whether their buffer can accommodate a large download before queueing it.

### Seed Time Shield
- Tracks seeding duration per torrent against community minimum seed-time rules (e.g., 72 hours).
- Provides visual indicators showing fulfilled and remaining seeding obligations to avoid Hit & Run penalties.

### Tracker Health Panel
- Tracks announce reachability, HTTP/UDP status codes, and countdown to next permitted announce.
