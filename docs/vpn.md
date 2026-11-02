# Nebulae VPN & Kill Switch Guide

## 1. Overview & Positioning

> **"Nebulae routes torrent traffic through your selected VPN where your operating system and network support it. Your other applications continue using their normal network connection. Nebulae does not collect or sell your activity data. VPN privacy depends on your VPN provider, DNS configuration, operating system, and network environment."**

Nebulae is designed to be completely provider-agnostic. Whether you use a commercial provider, a community VPN, or a self-hosted WireGuard instance, Nebulae allows you to isolate your torrent traffic onto the VPN tunnel without rerouting your entire operating system.

---

## 2. Operating Modes

Nebulae offers three selectable network modes under **Settings → VPN**:

1. **No VPN**: Direct connection using your default physical network interface (Ethernet or Wi-Fi).
2. **System VPN**: Uses whatever global VPN tunnel your operating system currently provides.
3. **Application-Only VPN**: Locks Nebulae's sockets strictly to the selected virtual VPN adapter (e.g. `wg0`, `utun3`, `OpenVPN TAP-Windows`). Other applications continue using your direct internet connection.

---

## 3. Importing VPN Profiles

### WireGuard (.conf)
1. Open **Settings → VPN → Profiles**.
2. Click **Import Profile** and select your WireGuard `.conf` file (or drag-and-drop the file into the window).
3. Nebulae extracts the interface address, endpoint, and routes.
4. The private key is immediately transferred into the **OS Keychain** (macOS Keychain or Windows Credential Manager). It is never saved in plaintext on disk or included in logs.

### OpenVPN (.ovpn / .ocfg)
1. Click **Import Profile** and select your `.ovpn` configuration file.
2. If username/password authentication is required, enter it once into the prompt; credentials are saved securely in the OS Keychain.
3. Nebulae configures socket binding for the OpenVPN network adapter.

---

## 4. The Nebulae Kill Switch

The integrated **Nebulae Kill Switch** protects your default network connection in the event of VPN tunnel failure:

- **How it works**: A background Tokio watcher monitors the bound VPN interface link state every 500ms.
- **On Disconnect**: If the VPN interface goes down, loses its IP address, or fails handshake renewal, Nebulae **instantly pauses all active torrents** and rebinds the BitTorrent engine socket to `127.0.0.1` (local loopback) to prevent socket leaks.
- **User Notice**: A high-visibility banner and status notice appears in the client:
  `VPN Disconnected — Transfers paused to protect your interface`
- **Reconnecting**: Transfers resume only after the VPN link is restored and verified, or when the user manually resumes transfers.

---

## 5. Scope, Verification & Limitations

Nebulae provides concrete, implementation-based network controls:

- **Interface Binding Verification**: The Connectivity Doctor runs periodic checks to confirm whether outgoing packets originate from your VPN IP address or your physical gateway.
- **Traffic Covered**: The application-only VPN setting binds the underlying BitTorrent engine (`transmission-daemon`) peer and transfer sockets directly to the specified VPN interface IPv4 address.
- **Traffic Not Covered**: Operating system traffic, third-party applications, external browser requests (e.g., clicking external documentation links), and standalone UI metadata requests (such as TMDB poster fetches) are not routed through the engine socket binding.
- **System DNS & IPv6 Scope**: Nebulae does not install virtual network kernel drivers or modify global OS routing tables. System DNS resolution and operating system IPv6 routing remain governed by your operating system and VPN provider configuration.
- **Default State**: By default, Nebulae operates in standard direct network mode ("No VPN"). Application-only VPN routing and Kill Switch enforcement require explicit user configuration or profile import.
