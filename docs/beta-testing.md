# Nebulae Beta Testing Guide

Welcome to the **Nebulae** beta testing program. Nebulae is engineered to provide reliable, high-scale seeding and deterministic networking for community BitTorrent users.

This guide outlines tester expectations, testing scenarios, feedback reporting, and data redaction requirements.

---

## 1. Beta Tester Expectations

As a beta tester, your feedback helps ensure rock-solid stability before general release. We encourage testing under both light daily desktop usage and heavy seeding loads.

### Key Goals
- **Stability**: Ensure the client does not crash, leak memory, or exhaust operating-system file descriptors under sustained multi-day seeding.
- **Accuracy**: Validate that UI ratio metrics, seed timers, and transfer rates accurately reflect engine activity.
- **Fail-Closed Networking**: Verify that the VPN kill switch and interface bindings properly cut transfer traffic when interfaces disconnect.
- **Performance**: Monitor responsiveness when managing hundreds or thousands of seeded torrents.

---

## 2. Recommended Test Scenarios

### Scenario A: High-Scale Seeding (Swarm Load)
1. Load 100 to 1,000+ torrents into Nebulae.
2. Monitor memory consumption (RSS) via Activity Monitor (macOS) or Task Manager (Windows).
3. Verify that the UI remains fluid and scrolling does not drop frames.
4. Confirm disk I/O remains stable without blocking the UI thread.

### Scenario B: Connectivity Doctor
1. Open **Connectivity Doctor** from the status bar or settings.
2. Run port reachability checks with UPnP enabled vs. disabled.
3. Test with and without router port forwarding configured.
4. Verify that the client accurately reflects "Local Listener Only (External Reachability Unverified)" when the port is not externally reachable from the outside.

### Scenario C: VPN Interface Binding & Kill Switch
1. In **VPN Settings**, bind Nebulae to a specific VPN interface (e.g., `utun3` on macOS or `WireGuard Tunnel` on Windows).
2. Start an active download or high-speed seed.
3. Disconnect or terminate the VPN tunnel.
4. **Verification**: Confirm that all transfer activity stops immediately and does not leak onto the default physical interface (Ethernet or Wi-Fi).

### Scenario D: Ratio Coach & Seed Time Shield
1. Inspect individual seeded torrents with active minimum seed time requirements (e.g. 72 hours).
2. Verify that the Seed Time Shield countdown calculates correctly.
3. Verify Ratio Coach target projections based on upload bandwidth allocation.

---

## 3. Strict Data Redaction Guidelines

When submitting bug reports, performance metrics, or diagnostic logs, **never expose sensitive credentials or private network information**:

| Sensitive Item | Guidance |
| :--- | :--- |
| **Passkeys & Tokens** | Always redact to `[REDACTED_PASSKEY]` or verify that the first 4 characters only are shown (`pass_****`). |
| **Private Tracker Announce URLs** | Replace private domains and passkey query parameters: `https://tracker.example.com/announce?passkey=[REDACTED]`. |
| **Public IP Addresses** | Redact public IPv4 and IPv6 addresses in logs (`198.51.100.xxx` or `2001:db8::xxxx`). |
| **WireGuard / OpenVPN Keys** | Never paste private keys, preshared keys, or authentication passwords. |

The built-in **Export Scrubbed Logs** function in *Der Sperre* (Advanced Diagnostics) automatically masks known passkey and IP patterns, but please double-check before posting publicly.

---

## 4. Submitting Feedback

To submit feedback:

1. Visit the GitHub Issues tab: [https://github.com/koppot/Nebulae-torrent_client/issues](https://github.com/koppot/Nebulae-torrent_client/issues)
2. Click **New Issue** and choose the **Beta Tester Feedback** template.
3. Provide your operating system version, hardware profile, active torrent count, and observed behavior.
4. For crash reports, include the crash stack trace and relevant lines from the scrubbed log export.

---

## 5. Community Participation

Join our testing and discussion threads at [Asian DVD Club](https://asiandvdclub.org). We appreciate your help in building the most reliable community BitTorrent client available.
