#!/usr/bin/env python3
"""
Installed application settings verification on macOS:
1. Selects Nebulae profile -> saves to DB -> launches installed app (/Applications/Nebulae.app)
   -> confirms active_engine_identity in DB becomes Nebulae (0.2.2, -NE0220-)
2. Restores Defaults (Transmission profile) -> saves to DB -> restarts installed app
   -> confirms active_engine_identity in DB becomes Transmission (4.1.3, -TR4130-)
3. Tests failed restart safeguard:
   Simulates daemon startup failure and confirms that active_engine_identity
   is NOT updated when the new daemon fails to become active.
"""

import json
import os
import shutil
import sqlite3
import subprocess
import sys
import time

APP_BINARY = "/Applications/Nebulae.app/Contents/MacOS/nebulae"
DAEMON_BINARY = "/Applications/Nebulae.app/Contents/Resources/resources/engine/transmission/transmission-daemon"
DB_PATH = os.path.expanduser("~/Library/Application Support/Nebulae/internal/db/nebulae.db")
ENGINE_STATE_DIR = os.path.expanduser("~/Library/Application Support/Nebulae/engine/transmission")

def get_db():
    return sqlite3.connect(DB_PATH)

def read_db_setting(key):
    conn = get_db()
    cursor = conn.cursor()
    cursor.execute("SELECT value FROM settings WHERE key = ?", (key,))
    row = cursor.fetchone()
    conn.close()
    return row[0] if row else None

def write_db_setting(key, value):
    conn = get_db()
    cursor = conn.cursor()
    cursor.execute("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)", (key, value))
    conn.commit()
    conn.close()

def kill_existing_processes():
    subprocess.run(["pkill", "-9", "-f", "Nebulae"], stderr=subprocess.DEVNULL)
    subprocess.run(["pkill", "-9", "-f", "transmission-daemon"], stderr=subprocess.DEVNULL)
    time.sleep(1.2)
    pid_file = os.path.join(ENGINE_STATE_DIR, "daemon.pid")
    if os.path.exists(pid_file):
        try:
            os.remove(pid_file)
        except Exception:
            pass

def wait_for_daemon_pid(timeout=15.0):
    start = time.time()
    pid_file = os.path.join(ENGINE_STATE_DIR, "daemon.pid")
    while time.time() - start < timeout:
        if os.path.isfile(pid_file):
            try:
                with open(pid_file, "r") as f:
                    pid = int(f.read().strip())
                    # Check if pid is running
                    os.kill(pid, 0)
                    return pid
            except Exception:
                pass
        time.sleep(0.2)
    return None

def test_scenario(name, profile, expected_profile, expected_prefix, expected_ua):
    print(f"\n========================================================")
    print(f" Installed App Check: {name}")
    print(f" Profile to set: '{profile}'")
    print(f"========================================================")

    kill_existing_processes()

    # Update user_settings_json with requested profile
    raw = read_db_setting("user_settings_json")
    settings = json.loads(raw) if raw else {}
    settings["engine_identity_profile"] = profile
    if profile == "transmission":
        settings["custom_user_agent"] = ""
        settings["custom_peer_id_prefix"] = ""
    write_db_setting("user_settings_json", json.dumps(settings))

    # Launch installed application
    app_proc = subprocess.Popen([APP_BINARY], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    print(f"  Launched {APP_BINARY} (pid {app_proc.pid})")

    daemon_pid = wait_for_daemon_pid(timeout=15.0)
    if not daemon_pid:
        print("  FAIL: transmission-daemon process did not start within timeout!")
        app_proc.terminate()
        kill_existing_processes()
        return False

    print(f"  transmission-daemon active (pid {daemon_pid})")

    # Wait briefly for RPC verification and DB active_engine_identity write
    passed = False
    start = time.time()
    while time.time() - start < 10.0:
        active_raw = read_db_setting("active_engine_identity")
        if active_raw:
            try:
                active_data = json.loads(active_raw)
                if active_data.get("profile") == expected_profile:
                    print(f"  Captured active_engine_identity in DB: {active_raw}")
                    actual_prefix = active_data.get("peer_id_prefix", "")
                    actual_ua = active_data.get("user_agent", "")
                    if actual_prefix.startswith(expected_prefix) and actual_ua == expected_ua:
                        print(f"  [PASS] Profile confirmed: {expected_profile}")
                        print(f"  [PASS] User-Agent confirmed: {actual_ua}")
                        print(f"  [PASS] Peer ID prefix confirmed: {actual_prefix}")
                        passed = True
                        break
            except Exception:
                pass
        time.sleep(0.3)

    if not passed:
        active_raw = read_db_setting("active_engine_identity")
        print(f"  FAIL: active_engine_identity mismatch or timed out! Value in DB: {active_raw}")

    # Clean termination
    app_proc.terminate()
    try:
        app_proc.wait(timeout=3.0)
    except Exception:
        app_proc.kill()
    kill_existing_processes()
    return passed

def test_failed_start_safeguard():
    print(f"\n========================================================")
    print(f" Testing Failed-Restart Safeguard")
    print(f"========================================================")

    kill_existing_processes()

    # Step 1: Ensure known baseline state (transmission)
    active_baseline = json.loads(read_db_setting("active_engine_identity") or "{}")
    baseline_profile = active_baseline.get("profile", "transmission")
    print(f"  Current baseline active profile in DB: {baseline_profile}")

    # Step 2: Set setting to 'nebulae'
    raw = read_db_setting("user_settings_json")
    settings = json.loads(raw) if raw else {}
    settings["engine_identity_profile"] = "nebulae"
    write_db_setting("user_settings_json", json.dumps(settings))

    # Step 3: Temporarily rename daemon so launch will fail
    bak_binary = DAEMON_BINARY + ".bak"
    shutil.move(DAEMON_BINARY, bak_binary)
    print(f"  Temporarily disabled daemon binary to simulate restart failure")

    try:
        app_proc = subprocess.Popen([APP_BINARY], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(3.0)
        app_proc.terminate()
        try:
            app_proc.wait(timeout=2.0)
        except Exception:
            app_proc.kill()
    finally:
        # Restore daemon binary
        shutil.move(bak_binary, DAEMON_BINARY)
        print(f"  Restored daemon binary")

    # Step 4: Verify active_engine_identity in DB was NOT updated to 'nebulae'
    active_after = json.loads(read_db_setting("active_engine_identity") or "{}")
    after_profile = active_after.get("profile")
    print(f"  Active profile in DB after failed launch: {after_profile}")

    if after_profile == baseline_profile and after_profile != "nebulae":
        print(f"  [PASS] Safeguard verified: failed launch did NOT update active_engine_identity to 'nebulae'.")
        return True
    else:
        print(f"  FAIL: active_engine_identity was modified despite failed restart!")
        return False

def main():
    if not os.path.isfile(APP_BINARY):
        print(f"ERROR: Installed application binary not found at: {APP_BINARY}")
        sys.exit(1)
    if not os.path.isfile(DB_PATH):
        print(f"ERROR: Nebulae DB not found at: {DB_PATH}")
        sys.exit(1)

    print(f"Testing installed application at: {APP_BINARY}")

    # 1. Select Nebulae -> save -> restart -> confirm active identity
    ok1 = test_scenario(
        name="Select Native Nebulae (v0.2.2)",
        profile="nebulae",
        expected_profile="nebulae",
        expected_prefix="-NE0220-",
        expected_ua="Nebulae/0.2.2",
    )

    # 2. Restore Defaults -> restart -> confirm Transmission
    ok2 = test_scenario(
        name="Restore Defaults (Transmission 4.1.3)",
        profile="transmission",
        expected_profile="transmission",
        expected_prefix="-TR4130-",
        expected_ua="Transmission/4.1.3",
    )

    # 3. Safeguard check
    ok3 = test_failed_start_safeguard()

    print("\n========================================================")
    if ok1 and ok2 and ok3:
        print(" ALL INSTALLED-APP CHECKS PASSED SUCCESSFULLY!")
        print(" 1. Select Nebulae -> restart -> confirmed active_engine_identity = nebulae / Nebulae/0.2.2 / -NE0220-")
        print(" 2. Restore Defaults -> restart -> confirmed active_engine_identity = transmission / Transmission/4.1.3 / -TR4130-")
        print(" 3. Failed launch safeguard confirmed: DB record only updates AFTER successful responsive startup.")
        print("========================================================")
        sys.exit(0)
    else:
        print(" INSTALLED-APP CHECKS FAILED!")
        print("========================================================")
        sys.exit(1)

if __name__ == "__main__":
    main()
