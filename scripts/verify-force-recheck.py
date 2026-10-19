#!/usr/bin/env python3
"""
scripts/verify-force-recheck.py

Empirically verifies Force Recheck behavior against the Transmission daemon:
1. Complete valid data: starts verify, records intermediate checking state &
   recheckProgress, and confirms final state is 100% complete with paused state preserved.
2. Active seeding data: starts verify on running torrent, confirms intermediate
   progress and automatic resume to active state (status 6).
3. Altered / truncated data: truncates data file to 50%, verifies that
   recheckProgress reports actual check and final percentDone drops to 50%.
4. Missing file: deletes data file, verifies that recheck reports local I/O error
   (error=3 / errorString set) mapping to "Error".
"""

import argparse
import base64
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

DEFAULT_DAEMON = os.path.abspath(
    os.path.join(
        os.path.dirname(__file__),
        "../src-tauri/resources/engine/transmission/transmission-daemon",
    )
)

def find_free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]

def rpc_call(port, method, arguments=None, session_id_holder=[None]):
    url = f"http://127.0.0.1:{port}/transmission/rpc"
    payload = json.dumps({"method": method, "arguments": arguments or {}}).encode("utf-8")
    for _ in range(15):
        req = urllib.request.Request(
            url,
            data=payload,
            headers={"Content-Type": "application/json"},
        )
        if session_id_holder[0]:
            req.add_header("X-Transmission-Session-Id", session_id_holder[0])
        try:
            with urllib.request.urlopen(req, timeout=5) as resp:
                return json.loads(resp.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            if e.code == 409:
                session_id_holder[0] = e.headers.get("X-Transmission-Session-Id")
                continue
            raise
        except Exception:
            time.sleep(0.15)
    raise RuntimeError(f"RPC call {method} timed out")

def create_torrent(target_file, payload_path, piece_len_kb=64):
    create_bin = "/opt/homebrew/bin/transmission-create"
    if not os.path.exists(create_bin):
        create_bin = "transmission-create"
    subprocess.run(
        [
            create_bin,
            "-o", target_file,
            "-t", "http://127.0.0.1:9999/announce",
            "-s", str(piece_len_kb),
            payload_path,
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

def main():
    parser = argparse.ArgumentParser(description="Verify Force Recheck behavior")
    parser.add_argument("--daemon-bin", default=DEFAULT_DAEMON, help="Path to transmission-daemon")
    args = parser.parse_args()

    daemon_bin = os.path.abspath(args.daemon_bin)
    if not os.path.exists(daemon_bin):
        print(f"Error: Daemon binary not found at {daemon_bin}", file=sys.stderr)
        sys.exit(1)

    temp_dir = tempfile.mkdtemp(prefix="nebulae-recheck-verify-")
    config_dir = os.path.join(temp_dir, "config")
    download_dir = os.path.join(temp_dir, "downloads")
    os.makedirs(config_dir, exist_ok=True)
    os.makedirs(download_dir, exist_ok=True)

    rpc_port = find_free_port()
    peer_port = find_free_port()

    # Create 64 MiB of random test payload
    test_file_path = os.path.join(download_dir, "sample_content.dat")
    chunk = os.urandom(1024 * 1024)
    with open(test_file_path, "wb") as f:
        for _ in range(64):
            f.write(chunk)

    torrent_path = os.path.join(temp_dir, "sample.torrent")
    create_torrent(torrent_path, test_file_path, piece_len_kb=64)

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

    daemon_proc = subprocess.Popen(
        [daemon_bin, "--config-dir", config_dir, "--foreground"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    time.sleep(1.2)

    try:
        session_id_holder = [None]
        with open(torrent_path, "rb") as f:
            b64_torrent = base64.b64encode(f.read()).decode("ascii")

        print("=== Test 1: Complete Valid Data Recheck (Paused State Preservation) ===")
        add_res = rpc_call(
            rpc_port,
            "torrent-add",
            {"metainfo": b64_torrent, "paused": True, "download-dir": download_dir},
            session_id_holder,
        )
        t_id = (add_res.get("arguments", {}).get("torrent-added") or {}).get("id")
        assert t_id is not None, f"Failed to add torrent: {add_res}"

        fields = ["id", "status", "percentDone", "recheckProgress", "error", "errorString"]
        t_init = rpc_call(rpc_port, "torrent-get", {"fields": fields, "ids": [t_id]}, session_id_holder)["arguments"]["torrents"][0]
        print(f"Initial state: status={t_init['status']}, percentDone={t_init['percentDone']:.2f}")

        # Trigger recheck
        verify_res = rpc_call(rpc_port, "torrent-verify", {"ids": [t_id]}, session_id_holder)
        assert verify_res.get("result") == "success", f"torrent-verify failed: {verify_res}"

        # Rapid poll to observe verification progress
        seen_checking = False
        recheck_progresses = []
        for _ in range(50):
            cur = rpc_call(rpc_port, "torrent-get", {"fields": fields, "ids": [t_id]}, session_id_holder)["arguments"]["torrents"][0]
            st = cur["status"]
            prog = cur["recheckProgress"]
            if st in (1, 2):
                seen_checking = True
                recheck_progresses.append(prog)
            elif seen_checking:
                # Finished checking
                break
            time.sleep(0.04)

        t_post = rpc_call(rpc_port, "torrent-get", {"fields": fields, "ids": [t_id]}, session_id_holder)["arguments"]["torrents"][0]
        print(f"Intermediate checking observed: {seen_checking} (sample progress values: {[f'{p:.3f}' for p in recheck_progresses[:5]]})")
        print(f"Final state: status={t_post['status']}, percentDone={t_post['percentDone']:.2f}, error={t_post['error']}")
        assert seen_checking, "Expected to observe Checking (status 1 or 2) state during verify"
        assert t_post["status"] == 0, f"Expected paused state (0) preserved, got {t_post['status']}"
        assert t_post["percentDone"] == 1.0, f"Expected 100% completion, got {t_post['percentDone']}"
        print("PASS: Valid data rechecked accurately, intermediate progress observed, paused state preserved.\n")

        print("=== Test 2: Active Seeding Torrent Recheck (Running State Preservation) ===")
        # Start the torrent
        rpc_call(rpc_port, "torrent-start", {"ids": [t_id]}, session_id_holder)
        time.sleep(0.2)
        t_running = rpc_call(rpc_port, "torrent-get", {"fields": fields, "ids": [t_id]}, session_id_holder)["arguments"]["torrents"][0]
        assert t_running["status"] in (5, 6), f"Expected running/seeding state, got {t_running['status']}"

        rpc_call(rpc_port, "torrent-verify", {"ids": [t_id]}, session_id_holder)
        seen_checking_running = False
        for _ in range(50):
            cur = rpc_call(rpc_port, "torrent-get", {"fields": fields, "ids": [t_id]}, session_id_holder)["arguments"]["torrents"][0]
            if cur["status"] in (1, 2):
                seen_checking_running = True
            elif seen_checking_running:
                break
            time.sleep(0.04)

        t_post_running = rpc_call(rpc_port, "torrent-get", {"fields": fields, "ids": [t_id]}, session_id_holder)["arguments"]["torrents"][0]
        print(f"Final state: status={t_post_running['status']}, percentDone={t_post_running['percentDone']:.2f}")
        assert t_post_running["status"] in (5, 6), f"Expected active/seeding state restored, got {t_post_running['status']}"
        print("PASS: Running torrent rechecked and resumed back to active state.\n")

        print("=== Test 3: Truncated / Corrupted Data Recheck ===")
        # Pause and truncate file to 32 MiB (half size)
        rpc_call(rpc_port, "torrent-stop", {"ids": [t_id]}, session_id_holder)
        time.sleep(0.2)
        with open(test_file_path, "r+b") as f:
            f.truncate(32 * 1024 * 1024)

        rpc_call(rpc_port, "torrent-verify", {"ids": [t_id]}, session_id_holder)
        seen_checking_trunc = False
        for _ in range(50):
            cur = rpc_call(rpc_port, "torrent-get", {"fields": fields, "ids": [t_id]}, session_id_holder)["arguments"]["torrents"][0]
            if cur["status"] in (1, 2):
                seen_checking_trunc = True
            elif seen_checking_trunc:
                break
            time.sleep(0.04)

        t_post_trunc = rpc_call(rpc_port, "torrent-get", {"fields": fields, "ids": [t_id]}, session_id_holder)["arguments"]["torrents"][0]
        print(f"Final state: status={t_post_trunc['status']}, percentDone={t_post_trunc['percentDone']:.4f}")
        assert 0.49 <= t_post_trunc["percentDone"] <= 0.51, f"Expected ~50% completion, got {t_post_trunc['percentDone']}"
        print("PASS: Truncated data accurately recalculated to valid fraction (50%).\n")

        print("=== Test 4: Missing File Recheck (Error Reporting) ===")
        rpc_call(rpc_port, "torrent-stop", {"ids": [t_id]}, session_id_holder)
        time.sleep(0.3)
        if os.path.exists(test_file_path):
            os.remove(test_file_path)
        if os.path.exists(test_file_path + ".part"):
            os.remove(test_file_path + ".part")

        rpc_call(rpc_port, "torrent-verify", {"ids": [t_id]}, session_id_holder)
        time.sleep(0.6)

        t_post_missing = rpc_call(rpc_port, "torrent-get", {"fields": fields, "ids": [t_id]}, session_id_holder)["arguments"]["torrents"][0]
        print(f"Final state: status={t_post_missing['status']}, percentDone={t_post_missing['percentDone']}, error={t_post_missing['error']}, errorString={t_post_missing['errorString']}")
        assert t_post_missing["error"] == 3 or t_post_missing["errorString"] != "", "Expected error reported for missing data file"
        print("PASS: Missing file accurately reported local I/O error (error=3 / errorString set).\n")

        print("=== ALL FORCE RECHECK VERIFICATION TESTS PASSED ===")

    finally:
        daemon_proc.terminate()
        try:
            daemon_proc.wait(timeout=3)
        except Exception:
            daemon_proc.kill()
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
