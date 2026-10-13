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
- **On Disconnect**: If the VPN interface goes down, loses its IP address, or fails handshake renewal, ADC **instantly pauses all active torrents**.
- **User Notice**: A high-visibility banner appears in the client:
  `VPN Disconnected — ADC Paused to protect your interface`
- **Reconnecting**: Transfers resume only after the VPN link is restored and verified, or when the user manually resumes transfers.

---

## 5. Leak Protection & Verification

ADC includes active safeguards to prevent data leaks:

- **DNS Leak Protection**: DNS queries for tracker announce URLs are directed through the VPN interface's designated DNS servers.
- **IPv6 Leak Protection**: If your imported VPN profile routes IPv4 only, ADC disables IPv6 peer connectivity for active torrents, preventing IPv6 fallback on your unencrypted connection.
- **Binding Verification**: The Connectivity Doctor runs periodic checks to confirm that outgoing packets originate from your VPN IP address and not your physical gateway.
