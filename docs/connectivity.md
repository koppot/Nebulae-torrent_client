# Nebulae Connectivity Doctor Guide

## 1. Overview & Positioning

> **"Nebulae helps you understand and improve your connection. The Connectivity Doctor checks your port, NAT, IPv6, DNS, VPN binding, and tracker connection, then provides clear steps to improve reliability. Nebulae cannot change restrictions imposed by routers, ISPs, workplaces, or other networks."**

Nebulae is designed to maximize transfer performance and swarm discoverability while providing clear, plain-language diagnostics for complex networking concepts.

---

## 2. The Connectivity Doctor

The Connectivity Doctor performs non-intrusive local and network probes to evaluate your connection health:

| Metric | Target State | Explanation |
| :--- | :--- | :--- |
| **Listening Port** | Reachable (Green) | Incoming peer connections can reach ADC on your configured port (default: `6881`). |
| **UPnP / NAT-PMP / PCP** | Active / Available | Automatic router port mapping protocols that open port forwarding leases on demand. |
| **NAT Traversal Type** | Full Cone / Moderate | Classifies whether your router allows unsolicited incoming packets from peers. |
| **IPv4 Connectivity** | Connected | Traditional Internet Protocol connectivity. |
| **IPv6 Connectivity** | Ready (Dual-Stack) | Modern, address-abundant protocol allowing direct peer connections without NAT. |
| **DNS Health** | Low Latency / Intact | DNS resolution speed and validation that tracker domains resolve cleanly. |
| **VPN Binding** | Securely Bound | Verifies that all ADC engine sockets are locked to the selected VPN network adapter. |
| **Tracker Status** | Healthy (200 OK) | Confirms HTTP and UDP tracker announces are succeeding without authentication faults. |

---

## 3. Interpreting Results

### Connectable vs. Non-Connectable
- **Connectable (Green Badge)**: Your port is reachable from the public internet. Both incoming and outgoing peers can connect to you. You will achieve maximum transfer speeds, rapid piece discovery, and optimal swarm health.
- **Firewalled / Non-Connectable (Yellow Badge)**: You can initiate outgoing connections to connectable peers, but peers cannot initiate connections to you. Transfers will work, but speed and available peer count may be reduced.

---

## 4. Guided Router Port-Forwarding Instructions

If UPnP is disabled on your router, follow these steps to manually forward port `6881`:

### General Instructions:
1. Open your browser and navigate to your router's gateway (typically `192.168.1.1` or `192.168.0.1`).
2. Log in with your router credentials.
3. Locate **Port Forwarding**, **Virtual Server**, or **NAT Rules** (usually under *Advanced* or *WAN* settings).
4. Add a new rule:
   - **Service Name**: `ADC BitTorrent`
   - **Protocol**: `TCP/UDP` (or `Both`)
   - **External Port**: `6881`
   - **Internal Port**: `6881`
   - **Internal IP**: Your computer's local IP address (e.g. `192.168.1.100`)
5. Click **Apply** or **Save**, then click the **Retest** button in ADC Connectivity Doctor.

---

## 5. ADC Connect Local Helper Service

**ADC Connect** is an optional standalone local background daemon that provides continuous connectivity management:

- **What ADC Connect Does**:
  - Automatically renews UPnP and PCP leases before they expire.
  - Monitors the active VPN interface and instantly signals ADC to pause downloads if the VPN drops.
  - Generates privacy-safe diagnostic reports for community support.
- **What ADC Connect Never Does**:
  - Never proxies torrent data through external cloud servers.
  - Never logs or stores IP addresses, torrent names, or passkeys.
  - Never modifies router or system settings without explicit user consent.
