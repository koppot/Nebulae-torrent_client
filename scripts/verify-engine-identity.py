#!/usr/bin/env python3
"""
Lightweight end-to-end verification of transmission-daemon engine identification.
Verifies:
1. Outgoing tracker announce HTTP User-Agent
2. Outgoing tracker announce peer_id query parameter (prefix + length 20)
3. Outgoing peer handshake peer_id (prefix + length 20)
4. Outgoing peer-facing extended handshake client/version string ("v" in BEP 10 LTEP)
5. Consistency between announce ID and handshake ID
6. Native Nebulae profile (Nebulae/0.2.2, -NE0220-)
7. Restore Defaults upstream Transmission profile (Transmission/4.1.3, -TR4130-)
"""

import argparse
import base64
import hashlib
import json
import os
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse
import urllib.request
from http.server import HTTPServer, BaseHTTPRequestHandler

class ReusableHTTPServer(HTTPServer):
    allow_reuse_address = True

def find_free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]

def make_tracker_handler(events_list):
    class MockTrackerHandler(BaseHTTPRequestHandler):
        def do_GET(self):
            parsed = urllib.parse.urlsplit(self.path)
            if parsed.path.startswith("/announce"):
                ua = self.headers.get("User-Agent", "")
                qs = urllib.parse.parse_qs(parsed.query)
                peer_id = qs.get("peer_id", [""])[0]
                events_list.append({
                    "user_agent": ua,
                    "peer_id": peer_id,
                })
                resp = b"d8:completei0e10:downloadedi0e10:incompletei0e8:intervali1800e12:min intervali300e5:peers0:e"
                self.send_response(200)
                self.send_header("Content-Type", "text/plain")
                self.send_header("Content-Length", str(len(resp)))
                self.end_headers()
                self.wfile.write(resp)
            else:
                self.send_response(404)
                self.end_headers()

        def log_message(self, format, *args):
            pass

    return MockTrackerHandler

def create_disposable_torrent(target_path, announce_url):
    piece_hash = hashlib.sha1(b"0" * 1024).digest()
    info_dict = (
        b"d6:lengthi1024e4:name8:test.bin12:piece lengthi16384e6:pieces20:" + piece_hash + b"e"
    )
    info_hash = hashlib.sha1(info_dict).digest()
    torrent_bytes = (
        b"d8:announce" + str(len(announce_url)).encode() + b":" + announce_url.encode() +
        b"4:info" + info_dict + b"e"
    )
    with open(target_path, "wb") as f:
        f.write(torrent_bytes)
    return info_hash

def rpc_call(rpc_port, method, arguments=None):
    url = f"http://127.0.0.1:{rpc_port}/transmission/rpc"
    payload = json.dumps({"method": method, "arguments": arguments or {}}).encode("utf-8")

    session_id = None
    for _ in range(10):
        req = urllib.request.Request(url, data=payload, headers={"Content-Type": "application/json"})
        if session_id:
            req.add_header("X-Transmission-Session-Id", session_id)
        try:
            with urllib.request.urlopen(req, timeout=3) as resp:
                return json.loads(resp.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            if e.code == 409:
                session_id = e.headers.get("X-Transmission-Session-Id")
                continue
            raise
        except Exception:
            time.sleep(0.3)
    raise RuntimeError("RPC call failed after retries")

def probe_peer_connection(peer_port, info_hash):
    """
    Connects to Transmission daemon's listening peer port, executes BitTorrent handshake,
    captures Transmission's 20-byte Peer ID, exchanges LTEP handshake, and extracts 'v'.
    """
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(3.0)
    s.connect(("127.0.0.1", peer_port))

    # Send BitTorrent handshake with BEP 10 extension bit enabled (reserved[5] |= 0x10)
    reserved = bytearray(8)
    reserved[5] |= 0x10
    my_peer_id = b"-MOCK01-123456789012"
    handshake = bytes([19]) + b"BitTorrent protocol" + bytes(reserved) + info_hash + my_peer_id
    s.sendall(handshake)

    # Receive Transmission's handshake (68 bytes)
    resp = b""
    while len(resp) < 68:
        chunk = s.recv(68 - len(resp))
        if not chunk:
            break
        resp += chunk

    if len(resp) < 68 or resp[0] != 19 or resp[1:20] != b"BitTorrent protocol":
        s.close()
        raise RuntimeError(f"Invalid handshake received from daemon: {resp[:20]}")

    peer_id_raw = resp[48:68]
    peer_id_str = peer_id_raw.decode("latin1", errors="replace")

    # Send our LTEP handshake
    # message id 20 (extended), extended message id 0 (handshake), bencoded dict
    payload = b"d1:md11:ut_metadatai1eee"
    msg = struct.pack(">IBB", len(payload) + 2, 20, 0) + payload
    s.sendall(msg)

    # Read daemon messages to find its LTEP extended handshake
    deadline = time.time() + 4.0
    buf = b""
    ltep_v = None

    while time.time() < deadline and ltep_v is None:
        try:
            chunk = s.recv(4096)
            if not chunk:
                break
            buf += chunk
        except socket.timeout:
            break

        while len(buf) >= 4:
            msg_len = struct.unpack(">I", buf[:4])[0]
            if msg_len == 0:  # keepalive
                buf = buf[4:]
                continue
            if len(buf) < 4 + msg_len:
                break  # wait for more
            msg_bytes = buf[4:4 + msg_len]
            buf = buf[4 + msg_len:]

            if msg_bytes[0] == 20 and len(msg_bytes) > 2 and msg_bytes[1] == 0:
                # Extended handshake dictionary
                ext_dict = msg_bytes[2:]
                # Look for '1:v' string in bencoded dict
                v_idx = ext_dict.find(b"1:v")
                if v_idx != -1:
                    colon_idx = ext_dict.find(b":", v_idx + 3)
                    if colon_idx != -1:
                        try:
                            v_len = int(ext_dict[v_idx + 3:colon_idx].decode("ascii"))
                            ltep_v = ext_dict[colon_idx + 1:colon_idx + 1 + v_len].decode("utf-8", errors="replace")
                        except Exception:
                            pass

    s.close()
    return peer_id_raw, peer_id_str, ltep_v

def run_test_scenario(scenario_name, daemon_bin, env_overrides, expected_ua, expected_prefix, expected_v):
    print(f"\n========================================================")
    print(f" Scenario: {scenario_name}")
    print(f" Expected UA:        {expected_ua}")
    print(f" Expected PeerID:    {expected_prefix} (8-byte prefix, exactly 20 bytes full)")
    print(f" Expected LTEP 'v':  {expected_v}")
    print(f"========================================================")

    tracker_port = find_free_port()
    rpc_port = find_free_port()
    peer_port = find_free_port()

    tracker_events = []
    handler_cls = make_tracker_handler(tracker_events)
    server = ReusableHTTPServer(("127.0.0.1", tracker_port), handler_cls)
    server.timeout = 0.5
    stop_tracker = threading.Event()

    def tracker_loop():
        while not stop_tracker.is_set():
            server.handle_request()
        server.server_close()

    tracker_th = threading.Thread(target=tracker_loop)
    tracker_th.daemon = True
    tracker_th.start()

    config_dir = tempfile.mkdtemp(prefix="nebu-test-cfg-")
    download_dir = tempfile.mkdtemp(prefix="nebu-test-dl-")
    torrent_file = os.path.join(config_dir, "test.torrent")
    info_hash = create_disposable_torrent(torrent_file, f"http://127.0.0.1:{tracker_port}/announce")

    settings = {
        "rpc-enabled": True,
        "rpc-port": rpc_port,
        "rpc-bind-address": "127.0.0.1",
        "rpc-whitelist-enabled": True,
        "rpc-whitelist": "127.0.0.1",
        "rpc-authentication-required": False,
        "peer-port": peer_port,
        "dht-enabled": False,
        "pex-enabled": False,
        "lpd-enabled": False,
        "download-dir": download_dir,
    }
    with open(os.path.join(config_dir, "settings.json"), "w") as f:
        json.dump(settings, f)

    env = dict(os.environ)
    env.pop("TRANSMISSION_USER_AGENT", None)
    env.pop("TRANSMISSION_PEER_ID_PREFIX", None)
    for k, v in env_overrides.items():
        env[k] = v

    proc = subprocess.Popen(
        [daemon_bin, "--config-dir", config_dir, "--foreground"],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    peer_id_raw = None
    peer_id_str = None
    ltep_v = None

    try:
        # Wait for daemon RPC to start
        time.sleep(1.5)

        # Add disposable torrent
        with open(torrent_file, "rb") as f:
            b64_torrent = base64.b64encode(f.read()).decode("ascii")

        rpc_call(rpc_port, "torrent-add", {"metainfo": b64_torrent, "paused": False})
        rpc_call(rpc_port, "torrent-start")

        # Wait for tracker announce
        deadline = time.time() + 8.0
        while time.time() < deadline:
            if tracker_events:
                break
            time.sleep(0.2)

        # Connect to daemon peer port to inspect peer handshake and LTEP extension
        for _ in range(10):
            try:
                peer_id_raw, peer_id_str, ltep_v = probe_peer_connection(peer_port, info_hash)
                if peer_id_str and ltep_v:
                    break
            except Exception:
                time.sleep(0.3)

    finally:
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except Exception:
            proc.kill()
        stop_tracker.set()
        shutil.rmtree(config_dir, ignore_errors=True)
        shutil.rmtree(download_dir, ignore_errors=True)

    if not tracker_events:
        print("  FAIL: No tracker announce request captured!")
        return False

    actual_ua = tracker_events[0]["user_agent"]
    actual_announce_peer_id = tracker_events[0]["peer_id"]

    print(f"  Captured Tracker User-Agent: '{actual_ua}'")
    print(f"  Captured Announce peer_id:    '{actual_announce_peer_id}' (len={len(actual_announce_peer_id)})")

    if not peer_id_str:
        print("  FAIL: No peer handshake captured!")
        return False

    print(f"  Captured Handshake peer_id:   '{peer_id_str}' (bytes={len(peer_id_raw)})")
    print(f"  Captured Peer LTEP 'v':       '{ltep_v}'")

    passed = True
    if actual_ua != expected_ua:
        print(f"  MISMATCH: User-Agent expected '{expected_ua}', got '{actual_ua}'")
        passed = False
    else:
        print(f"  [PASS] Tracker HTTP User-Agent matches: {actual_ua}")

    if not actual_announce_peer_id.startswith(expected_prefix):
        print(f"  MISMATCH: Announce peer_id '{actual_announce_peer_id}' does not start with '{expected_prefix}'")
        passed = False
    else:
        print(f"  [PASS] Announce peer_id prefix matches: {expected_prefix}")

    if len(actual_announce_peer_id) != 20:
        print(f"  MISMATCH: Announce peer_id length is {len(actual_announce_peer_id)}, expected 20")
        passed = False
    else:
        print(f"  [PASS] Announce peer_id is exactly 20 bytes")

    if not peer_id_str.startswith(expected_prefix):
        print(f"  MISMATCH: Handshake peer_id '{peer_id_str}' does not start with '{expected_prefix}'")
        passed = False
    else:
        print(f"  [PASS] Handshake peer_id prefix matches: {expected_prefix}")

    if len(peer_id_raw) != 20:
        print(f"  MISMATCH: Handshake peer_id bytes length is {len(peer_id_raw)}, expected 20")
        passed = False
    else:
        print(f"  [PASS] Handshake peer_id is exactly 20 bytes")

    if actual_announce_peer_id != peer_id_str:
        print(f"  MISMATCH: Announce peer_id ('{actual_announce_peer_id}') and Handshake peer_id ('{peer_id_str}') do not agree!")
        passed = False
    else:
        print(f"  [PASS] Announce peer_id and Handshake peer_id AGREE: '{actual_announce_peer_id}'")

    if ltep_v != expected_v:
        print(f"  MISMATCH: LTEP 'v' expected '{expected_v}', got '{ltep_v}'")
        passed = False
    else:
        print(f"  [PASS] Peer-facing extended handshake 'v' string matches: {ltep_v}")

    return passed

def main():
    parser = argparse.ArgumentParser(description="End-to-end verification of transmission-daemon engine identification")
    parser.add_argument("--daemon-bin", help="Path to transmission-daemon binary to test")
    args = parser.parse_args()

    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
    default_daemon = os.path.join(repo_root, "src-tauri/resources/engine/transmission/transmission-daemon")
    if sys.platform == "win32":
        default_daemon += ".exe"

    daemon_bin = args.daemon_bin or default_daemon
    if not os.path.isfile(daemon_bin):
        print(f"ERROR: Packaged transmission-daemon binary not found at: {daemon_bin}")
        sys.exit(1)

    print(f"Testing packaged daemon: {daemon_bin}")

    ok1 = run_test_scenario(
        scenario_name="Native Nebulae Identity (v0.2.2)",
        daemon_bin=daemon_bin,
        env_overrides={
            "TRANSMISSION_USER_AGENT": "Nebulae/0.2.2",
            "TRANSMISSION_PEER_ID_PREFIX": "-NE0220-",
        },
        expected_ua="Nebulae/0.2.2",
        expected_prefix="-NE0220-",
        expected_v="Nebulae/0.2.2",
    )

    ok2 = run_test_scenario(
        scenario_name="Restore Defaults (Upstream Transmission 4.1.3)",
        daemon_bin=daemon_bin,
        env_overrides={},
        expected_ua="Transmission/4.1.3",
        expected_prefix="-TR4130-",
        expected_v="Transmission 4.1.3",
    )

    print("\n========================================================")
    if ok1 and ok2:
        print(" ALL VERIFICATIONS PASSED SUCCESSFULLY!")
        print(" 1. Tracker announce HTTP User-Agent: VERIFIED")
        print(" 2. Announce peer_id prefix and length (20 bytes): VERIFIED")
        print(" 3. Peer handshake peer_id prefix and length (20 bytes): VERIFIED")
        print(" 4. Peer-facing LTEP handshake 'v' string: VERIFIED")
        print(" 5. Announce and handshake peer_ids agree: VERIFIED")
        print(" 6. Restore Defaults returns upstream Transmission values: VERIFIED")
        print("========================================================")
        sys.exit(0)
    else:
        print(" VERIFICATION FAILED!")
        print("========================================================")
        sys.exit(1)

if __name__ == "__main__":
    main()
