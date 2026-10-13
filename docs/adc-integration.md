# AsianDVDClub Integration Framework

## 1. Overview & Community Alignment

ADC provides native, official integration with **AsianDVDClub** (`asiandvdclub.org`), designed to support archival preservation, community media collections, and tracker health.

ADC interacts exclusively with official, documented, tracker-approved APIs. ADC **never scrapes web pages**, calls undocumented endpoints, or attempts to bypass tracker authentication rules.

---

## 2. Passkey Security & Storage

- **OS Keychain Integration**: When you link your AsianDVDClub profile, your personal passkey is stored securely in the operating system's native credential store:
  - **macOS**: Apple Keychain Services (`Security.framework`)
  - **Windows**: Windows Credential Manager (`advapi32.dll`)
- **Masking in UI**: Passkeys are always masked in the interface (e.g. `adc_pk_••••••••••••••••3a9f`).
- **Zero Logging Guarantee**: The internal `SafeLogger` automatically redacts passkeys, authentication parameters, and announce query strings from all session logs and diagnostic exports.

---

## 3. Private Swarm Safeguards (BEP 27)

When a `.torrent` file contains the private flag (`info.private = 1`):

1. **DHT is permanently disabled** for that torrent.
2. **Peer Exchange (PEX) is permanently disabled** for that torrent.
3. **Local Service Discovery (LSD) is permanently disabled** for that torrent.
4. **Tracker Exclusivity**: Peer discovery is conducted strictly through the authenticated AsianDVDClub announce URL.

These safeguards ensure strict compliance with private community rules, prevent passkey leakage across swarms, and protect upload/download accounting accuracy.

---

## 4. Community Health & Account Features

### Ratio Dashboard
- Real-time display of your lifetime uploaded and downloaded totals.
- Live share ratio calculation ($R = \frac{\text{Uploaded}}{\text{Downloaded}}$).
- Official freeleech badge indicators and community bonus points (when supported by official APIs).

### Seed Time Shield
- Tracks your seeding duration per torrent against community minimum seed time rules (e.g. 72 hours minimum).
- Warns before stopping or removing a torrent if minimum seeding time has not yet been satisfied, helping you avoid hit-and-run warnings.

### Ratio Coach
- Proactive ratio impact calculator.
- When adding a large multi-gigabyte release, Ratio Coach calculates the projected impact on your overall share ratio and advises whether your buffer is sufficient.

### Tracker Health Panel
- Live announce status and HTTP/UDP response codes.
- Precise countdown timer to your next permitted tracker announce.
