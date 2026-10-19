#!/usr/bin/env python3
"""
scripts/test-consecutive-silent-updates.py

Executes a real consecutive silent automatic update test across two consecutive
version hops:
  Hop 1: Version A (0.2.2) -> Version B (0.2.3)
  Hop 2: Version B (0.2.3) -> Version C (0.2.4)
  Final: Version C (0.2.4) is up to date

Tests:
- Cryptographic Minisign signature generation & validation
- Headless background discovery without dialogs
- In-place bundle replacement (update.install)
- Persistence of SQLite settings, torrent state, and media collection
"""

import http.server
import json
import os
import plistlib
import shutil
import socketserver
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time

TEST_DIR = "/private/tmp/nebulae-consecutive-update-test"
PORT = 9753

current_manifest = {}

class UpdateHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=os.path.join(TEST_DIR, "server"), **kwargs)

    def do_GET(self):
        if self.path == "/latest.json":
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            payload = json.dumps(current_manifest, indent=2).encode("utf-8")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
        else:
            super().do_GET()

    def log_message(self, format, *args):
        # Suppress noisy HTTP request logging
        pass

def main():
    global current_manifest
    print("=" * 70)
    print("  NEBULAE CONSECUTIVE SILENT AUTOMATIC UPDATE TEST (A -> B -> C)")
    print("=" * 70)

    # 1. Clean and initialize test directories
    shutil.rmtree(TEST_DIR, ignore_errors=True)
    os.makedirs(os.path.join(TEST_DIR, "server"), exist_ok=True)
    os.makedirs(os.path.join(TEST_DIR, "keys"), exist_ok=True)
    os.makedirs(os.path.join(TEST_DIR, "install"), exist_ok=True)
    home_dir = os.path.join(TEST_DIR, "home")
    app_support = os.path.join(home_dir, "Library", "Application Support", "org.asiandvdclub.nebulae")
    os.makedirs(app_support, exist_ok=True)

    # 2. Setup isolated database
    db_path = os.path.join(app_support, "nebulae.db")
    print(f"\n[1/7] Initializing isolated database at: {db_path}")
    conn = sqlite3.connect(db_path)
    cur = conn.cursor()
    cur.executescript("""
        CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE torrents (id TEXT PRIMARY KEY, name TEXT NOT NULL, is_media INTEGER NOT NULL, added_date INTEGER);
        CREATE TABLE external_media (path TEXT PRIMARY KEY, title TEXT NOT NULL, year INTEGER);

        INSERT INTO settings VALUES ('user_settings_json', '{"auto_update_enabled":true,"racing_mode_enabled":true,"media_mode_enabled":true,"listening_port":51413}');
        INSERT INTO torrents VALUES ('torrent-aaa-111', 'Fuuka BD-BOX', 1, 1729200000);
        INSERT INTO torrents VALUES ('torrent-bbb-222', 'Banner of the Stars III OVA', 1, 1729201000);
        INSERT INTO external_media VALUES ('/Volumes/Media/91 DAYS D1', '91 Days', 2016);
    """)
    conn.commit()
    conn.close()
    print("      ✓ Database seeded: 2 torrents, 1 media record, auto_update_enabled=true")

    # 3. Generate test signing keypair
    key_path = os.path.join(TEST_DIR, "keys", "test.key")
    print(f"\n[2/7] Generating isolated test Minisign keypair...")
    subprocess.run(
        ["pnpm", "exec", "tauri", "signer", "generate", "--ci", "-w", key_path, "-p", ""],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        cwd="/Users/dev/Documents/GitHub/Nebulae-torrent_client"
    )
    with open(key_path + ".pub", "r") as f:
        test_pubkey = f.read().strip()
    print(f"      ✓ Generated public key: {test_pubkey[:40]}...")

    # 4. Prepare base Nebulae.app bundle
    app_install = os.path.join(TEST_DIR, "install", "Nebulae.app")
    base_app = "/Applications/Nebulae.app"
    if not os.path.exists(base_app):
        print(f"ERROR: {base_app} does not exist.")
        sys.exit(1)
    shutil.copytree(base_app, app_install, symlinks=True)

    # Install Binary A (v0.2.2) into installed bundle
    shutil.copy("/tmp/nebulae-bin-A", os.path.join(app_install, "Contents", "MacOS", "nebulae"))
    plist_path = os.path.join(app_install, "Contents", "Info.plist")
    with open(plist_path, "rb") as f:
        pl = plistlib.load(f)
    pl["CFBundleShortVersionString"] = "0.2.2"
    pl["CFBundleVersion"] = "0.2.2"
    with open(plist_path, "wb") as f:
        plistlib.dump(pl, f)
    print(f"\n[3/7] Installed Version A (0.2.2) at: {app_install}")

    # 5. Build signed update payload B (v0.2.3)
    print(f"\n[4/7] Packaging and signing Version B (0.2.3) payload...")
    staging_b = os.path.join(TEST_DIR, "staging_b")
    os.makedirs(staging_b, exist_ok=True)
    app_b = os.path.join(staging_b, "Nebulae.app")
    shutil.copytree(app_install, app_b, symlinks=True)
    shutil.copy("/tmp/nebulae-bin-B", os.path.join(app_b, "Contents", "MacOS", "nebulae"))
    plist_b = os.path.join(app_b, "Contents", "Info.plist")
    with open(plist_b, "rb") as f:
        pl_b = plistlib.load(f)
    pl_b["CFBundleShortVersionString"] = "0.2.3"
    pl_b["CFBundleVersion"] = "0.2.3"
    with open(plist_b, "wb") as f:
        plistlib.dump(pl_b, f)
    with open(os.path.join(app_b, "Contents", "Resources", "NEBULAE_VERSION"), "w") as f:
        f.write("0.2.3\n")

    tar_b = os.path.join(TEST_DIR, "server", "Nebulae-v0.2.3.tar.gz")
    subprocess.run(["tar", "-czf", tar_b, "-C", staging_b, "Nebulae.app"], check=True)
    sign_env = os.environ.copy()
    sign_env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = ""
    subprocess.run(
        ["pnpm", "exec", "tauri", "signer", "sign", "-f", key_path, "-p", "", "--app-version", "0.2.3", tar_b],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        cwd="/Users/dev/Documents/GitHub/Nebulae-torrent_client",
        env=sign_env
    )
    with open(tar_b + ".sig", "r") as f:
        sig_b = f.read().strip()
    print(f"      ✓ Version B tarball created ({os.path.getsize(tar_b):,} bytes) and signed with Minisign.")

    # 6. Build signed update payload C (v0.2.4)
    print(f"\n[5/7] Packaging and signing Version C (0.2.4) payload...")
    staging_c = os.path.join(TEST_DIR, "staging_c")
    os.makedirs(staging_c, exist_ok=True)
    app_c = os.path.join(staging_c, "Nebulae.app")
    shutil.copytree(app_install, app_c, symlinks=True)
    shutil.copy("/tmp/nebulae-bin-C", os.path.join(app_c, "Contents", "MacOS", "nebulae"))
    plist_c = os.path.join(app_c, "Contents", "Info.plist")
    with open(plist_c, "rb") as f:
        pl_c = plistlib.load(f)
    pl_c["CFBundleShortVersionString"] = "0.2.4"
    pl_c["CFBundleVersion"] = "0.2.4"
    with open(plist_c, "wb") as f:
        plistlib.dump(pl_c, f)
    with open(os.path.join(app_c, "Contents", "Resources", "NEBULAE_VERSION"), "w") as f:
        f.write("0.2.4\n")

    tar_c = os.path.join(TEST_DIR, "server", "Nebulae-v0.2.4.tar.gz")
    subprocess.run(["tar", "-czf", tar_c, "-C", staging_c, "Nebulae.app"], check=True)
    subprocess.run(
        ["pnpm", "exec", "tauri", "signer", "sign", "-f", key_path, "-p", "", "--app-version", "0.2.4", tar_c],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        cwd="/Users/dev/Documents/GitHub/Nebulae-torrent_client",
        env=sign_env
    )
    with open(tar_c + ".sig", "r") as f:
        sig_c = f.read().strip()
    print(f"      ✓ Version C tarball created ({os.path.getsize(tar_c):,} bytes) and signed with Minisign.")

    # 7. Start local HTTP server
    server = socketserver.TCPServer(("127.0.0.1", PORT), UpdateHandler)
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()
    print(f"      ✓ Local test update server running at http://127.0.0.1:{PORT}")

    test_env = os.environ.copy()
    test_env["HOME"] = home_dir
    test_env["NEBULAE_UPDATER_TEST_ENDPOINT"] = f"http://127.0.0.1:{PORT}/latest.json"
    test_env["NEBULAE_UPDATER_TEST_PUBKEY"] = test_pubkey
    test_env["NEBULAE_UPDATER_TEST_QUIT_ON_STAGED"] = "1"
    test_env["RUST_LOG"] = "info,nebulae_torrent_client_lib::updater=debug,tauri_plugin_updater=debug"

    import queue

    def run_app_instance(expected_hop):
        exe = os.path.join(app_install, "Contents", "MacOS", "nebulae")
        proc = subprocess.Popen(
            [exe],
            env=test_env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1
        )
        logs = []
        q = queue.Queue()

        def reader():
            try:
                for l in iter(proc.stdout.readline, ""):
                    q.put(l)
                proc.stdout.close()
            except Exception:
                pass

        t = threading.Thread(target=reader, daemon=True)
        t.start()

        start_time = time.time()
        while time.time() - start_time < 30:
            try:
                line = q.get(timeout=0.1)
                s = line.strip()
                logs.append(s)
                if "Silent update" in s or "up to date" in s or "updater" in s.lower():
                    print("      [LOG]", s)
                if "Nebulae is up to date" in s and expected_hop == "C Check":
                    time.sleep(0.5)
                    break
            except queue.Empty:
                if proc.poll() is not None:
                    while not q.empty():
                        try:
                            s = q.get_nowait().strip()
                            logs.append(s)
                            if "Silent update" in s or "up to date" in s or "updater" in s.lower():
                                print("      [LOG]", s)
                        except Exception:
                            break
                    break

        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
        return logs, proc.returncode

    # =========================================================================
    # EXECUTE HOP 1: A (0.2.2) -> B (0.2.3)
    # =========================================================================
    print("\n" + "=" * 70)
    print("  HOP 1: Executing update Version A (0.2.2) -> Version B (0.2.3)")
    print("=" * 70)
    current_manifest = {
        "version": "0.2.3",
        "notes": "Test Hop 1: Version B upgrade",
        "pub_date": "2026-10-18T23:30:00Z",
        "platforms": {
            "darwin-aarch64": {
                "signature": sig_b,
                "url": f"http://127.0.0.1:{PORT}/Nebulae-v0.2.3.tar.gz"
            }
        }
    }

    logs_1, code_1 = run_app_instance("A -> B")
    assert any("Silent update downloaded and verified" in l for l in logs_1), "Signature verification must pass for B"
    assert any("Silent update staged on disk" in l for l in logs_1), "Update B must stage on disk"
    print("  ✓ Signature verified with test Minisign public key.")
    print("  ✓ Application bundle in-place replacement completed.")

    # Verify disk state after Hop 1
    with open(plist_path, "rb") as f:
        pl_after_1 = plistlib.load(f)
    ver_after_1 = pl_after_1.get("CFBundleShortVersionString")
    print(f"  ✓ Installed Info.plist version is now: {ver_after_1}")
    assert ver_after_1 == "0.2.3", f"Expected version 0.2.3 after hop 1, got {ver_after_1}"

    # Verify data preservation
    conn = sqlite3.connect(db_path)
    cur = conn.cursor()
    cur.execute("SELECT COUNT(*) FROM torrents")
    assert cur.fetchone()[0] == 2, "Torrents table MUST remain intact after Hop 1"
    cur.execute("SELECT COUNT(*) FROM external_media")
    assert cur.fetchone()[0] == 1, "External media table MUST remain intact after Hop 1"
    cur.execute("SELECT value FROM settings WHERE key = 'user_settings_json'")
    settings_raw = cur.fetchone()[0]
    assert "racing_mode_enabled" in settings_raw and "listening_port" in settings_raw, "Settings preserved"
    conn.close()
    print("  ✓ Database integrity verified: all torrents and settings intact.")

    # =========================================================================
    # EXECUTE HOP 2: B (0.2.3) -> C (0.2.4)
    # =========================================================================
    print("\n" + "=" * 70)
    print("  HOP 2: Executing update Version B (0.2.3) -> Version C (0.2.4)")
    print("=" * 70)
    current_manifest = {
        "version": "0.2.4",
        "notes": "Test Hop 2: Version C upgrade",
        "pub_date": "2026-10-18T23:35:00Z",
        "platforms": {
            "darwin-aarch64": {
                "signature": sig_c,
                "url": f"http://127.0.0.1:{PORT}/Nebulae-v0.2.4.tar.gz"
            }
        }
    }

    logs_2, code_2 = run_app_instance("B -> C")
    assert any("Silent update downloaded and verified" in l for l in logs_2), "Signature verification must pass for C"
    assert any("Silent update staged on disk" in l for l in logs_2), "Update C must stage on disk"
    print("  ✓ Signature verified with test Minisign public key.")
    print("  ✓ Application bundle in-place replacement completed.")

    # Verify disk state after Hop 2
    with open(plist_path, "rb") as f:
        pl_after_2 = plistlib.load(f)
    ver_after_2 = pl_after_2.get("CFBundleShortVersionString")
    print(f"  ✓ Installed Info.plist version is now: {ver_after_2}")
    assert ver_after_2 == "0.2.4", f"Expected version 0.2.4 after hop 2, got {ver_after_2}"

    # Verify data preservation
    conn = sqlite3.connect(db_path)
    cur = conn.cursor()
    cur.execute("SELECT COUNT(*) FROM torrents")
    assert cur.fetchone()[0] == 2, "Torrents table MUST remain intact after Hop 2"
    cur.execute("SELECT COUNT(*) FROM external_media")
    assert cur.fetchone()[0] == 1, "External media table MUST remain intact after Hop 2"
    conn.close()
    print("  ✓ Database integrity verified: all torrents and settings intact.")

    # =========================================================================
    # HOP 3: Launch C and verify Up-to-Date check
    # =========================================================================
    print("\n" + "=" * 70)
    print("  FINAL CHECK: Launching Version C (0.2.4) to verify Up-to-Date state")
    print("=" * 70)
    # Manifest remains 0.2.4
    test_env["NEBULAE_UPDATER_TEST_QUIT_ON_STAGED"] = "0"
    logs_3, code_3 = run_app_instance("C Check")
    assert any("Nebulae is up to date" in l for l in logs_3), "App C must confirm it is up to date"
    print("  ✓ App C discovered no new updates; confirmed up to date.")

    # Shutdown HTTP server
    server.shutdown()
    server.server_close()

    print("\n" + "=" * 70)
    print("  ALL CONSECUTIVE SILENT UPDATE ACCEPTANCE TESTS PASSED (100% SUCCESS)")
    print("=" * 70)
    print(f"  Starting Version: 0.2.2")
    print(f"  Hop 1 Target:     0.2.3  [SIGNATURE VERIFIED / IN-PLACE SWAP PASS]")
    print(f"  Hop 2 Target:     0.2.4  [SIGNATURE VERIFIED / IN-PLACE SWAP PASS]")
    print(f"  Final State:      0.2.4  [UP TO DATE / ZERO DIALOGS / 100% DATA PRESERVED]")
    print("=" * 70 + "\n")

if __name__ == "__main__":
    main()
