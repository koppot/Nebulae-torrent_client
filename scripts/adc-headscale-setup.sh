#!/usr/bin/env bash
# ADC server: install + configure the Headscale control plane and the
# server's own tailscaled node. REVIEW-ONLY until the owner approves
# deployment; running it on the wrong box changes nothing (checks below).
#
# Requirements: Debian/Ubuntu w/ root, existing nginx/caddy + certbot,
# outbound internet.
set -euo pipefail

# --- Pins (bump together with the client's bundled tailscale version) ------
HEADSCALE_VERSION="0.29.5"     # github.com/juanfont/headscale
TAILSCALE_VERSION="1.104.1"    # pkgs.tailscale.com
CONTROL_HOST="control.asiandvdclub.org"
ADC_USER="club"

[[ $EUID -eq 0 ]] || { echo "run as root"; exit 1; }
# Refuse to run on the wrong machine: the control host must resolve here.
getent hosts "$CONTROL_HOST" >/dev/null || {
  echo "REFUSING: $CONTROL_HOST does not resolve to this host. Run on the ADC server after DNS is set up."; exit 1;
}

# --- headscale install (pinned) ---------------------------------------------
curl -fsSL -o /usr/local/bin/headscale \
  "https://github.com/juanfont/headscale/releases/download/v${HEADSCALE_VERSION}/headscale_${HEADSCALE_VERSION}_linux_amd64"
chmod 0755 /usr/local/bin/headscale

id headscale >/dev/null 2>&1 || useradd --system --home /var/lib/headscale headscale
mkdir -p /etc/headscale /var/lib/headscale /var/run/headscale
chown headscale:headscale /var/lib/headscale /var/run/headscale

cat > /etc/headscale/config.yaml <<CFG
server_url: https://${CONTROL_HOST}
listen_addr: 127.0.0.1:8080
unix_socket: /var/run/headscale/headscale.sock
unix_socket_permission: 0770
metrics_listen_addr: 127.0.0.1:9090
grpc_listen_addr: 127.0.0.1:50443
grpc_allow_insecure: false
noise:
  private_key_path: /var/lib/headscale/noise_private.key
prefixes:
  v4: 100.64.0.0/10
  v6: fd7a:115c:a1e0::/48
derp:
  server_enabled: false            # day one: public DERP fallback; revisit for privacy
  urls: ["https://controlplane.tailscale.com/derpmap/default"]
  auto_update_enabled: true
  update_frequency: 24h
dns:
  magic_dns: true
  base_domain: adc.asiandvdclub.org
  nameservers:
    global: ["1.1.1.1"]
private_key_path: /var/lib/headscale/private.key
database:
  type: sqlite
  sqlite:
    path: /var/lib/headscale/db.sqlite
CFG

cat > /etc/systemd/system/headscale.service <<UNIT
[Unit]
Description=headscale (ADC club Tailscale control plane)
After=network-online.target
[Service]
User=headscale
ExecStart=/usr/local/bin/headscale serve --config /etc/headscale/config.yaml
Restart=always
RestartSec=5
[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload && systemctl enable --now headscale

# --- club user + single-use enrollment keys ---------------------------------
headscale users create "$ADC_USER" || true

# Issuing keys is a per-member, logged-in-website action (cron/portal), e.g.:
#   headscale preauthkeys create --user $(id -u headscale >/dev/null 2>&1 && echo 1) \
#     --expiration 30m   # SINGLE-USE by default: one key = one device
# (owner decision: never ship a shared long-lived club key)

# --- server's own node (subnet router) --------------------------------------
curl -fsSL "https://pkgs.tailscale.com/stable/tailscale_${TAILSCALE_VERSION}_amd64.tgz" | tar xz -C /tmp
cp "/tmp/tailscale_${TAILSCALE_VERSION}_amd64/tailscale" "/tmp/tailscale_${TAILSCALE_VERSION}_amd64/tailscaled" /usr/local/bin/
tailscaled --tun=userspace-networking --statedir=/var/lib/tailscale-adc --port=0 \
  --socks5-server=127.0.0.1:1055 & # userspace: server needs no TUN either
sleep 2
# Enrollment of the server node is INTERACTIVE (prints a URL the admin
# approves in headscale) — deliberately not scripted:
echo "NEXT: run as admin:  tailscale --socket /var/run/tailscale/tailscaled.sock up \\
  --login-server https://${CONTROL_HOST} --hostname=asiandvdclub \\
  --advertise-routes=<SERVER_PUBLIC_SERVICE_IPS>"
echo "then approve routes:  headscale nodes list && headscale routes enable"

# --- backups -----------------------------------------------------------------
echo "/var/lib/headscale /var/backups  none-bind  optional" # integrate with existing backup job
