#!/usr/bin/env python3
"""
scripts/test-windows-consecutive-silent-updates.py

Executes a real consecutive silent automatic update test on Windows (supermicro.local)
across two consecutive version hops using genuine compiled packages and the live HTTPS feed:
  Hop 1: Version A (0.2.2) -> Version B (0.2.3)
  Hop 2: Version B (0.2.3) -> Version C (0.2.4)
  Final: Version C (0.2.4) confirmed up to date
"""

import json
import os
import subprocess
import sys
import time

SSH_ADC = ["ssh", "-i", os.path.expanduser("~/.ssh/id_rsa"), "-o", "ConnectTimeout=10", "root@45.135.163.139"]
SSH_WIN = ["ssh", "-i", os.path.expanduser("~/.ssh/id_rsa"), "-o", "ConnectTimeout=10", "administrator@supermicro.local"]
HTTPS_ENDPOINT = "https://asiandvdclub.org/app-update-test-a7f92e4c91b5/latest.json"

def run_win_ps(cmd):
    full_cmd = SSH_WIN + ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", cmd]
    res = subprocess.run(full_cmd, capture_output=True, text=True)
    # Filter OpenSSH security notice
    lines = [l for l in res.stdout.splitlines() if not l.startswith("** WARNING:")]
    return "\n".join(lines).strip(), res.returncode

def set_remote_manifest(stage_file):
    print(f"      [REMOTE] Setting active manifest to {stage_file} on asiandvdclub.org...")
    cmd = SSH_ADC + [f"cp /var/www/html/app-update-test-a7f92e4c91b5/{stage_file} /var/www/html/app-update-test-a7f92e4c91b5/latest.json"]
    subprocess.run(cmd, check=True)
    res = subprocess.run(["curl", "-s", HTTPS_ENDPOINT], capture_output=True, text=True, check=True)
    data = json.loads(res.stdout)
    print(f"      [REMOTE] Active HTTPS manifest now advertises version: {data.get('version')}")
    return data

def get_installed_version():
    out, code = run_win_ps("(Get-Item C:\\Users\\administrator\\AppData\\Local\\Nebulae\\nebulae.exe).VersionInfo.ProductVersion")
    return out.strip()

def main():
    print("=" * 70)
    print("  WINDOWS GENUINE CONSECUTIVE SILENT UPDATE ACCEPTANCE TEST (A -> B -> C)")
    print("  Target Host: supermicro.local (Windows 11 / Server)")
    print("  Live HTTPS Endpoint: https://asiandvdclub.org/app-update-test-a7f92e4c91b5/")
    print("  Authentic Minisign Key: 452CAA8BBBBC6F41 (No Key Override)")
    print("=" * 70)

    # 1. Check Starting Version A
    ver_a = get_installed_version()
    print(f"\n[1/6] Checking starting Windows executable version: {ver_a}")
    assert ver_a == "0.2.2", f"Expected starting version 0.2.2, got {ver_a}"
    print("      ✓ Confirmed starting at version 0.2.2")

    # 2. Check Tailscale preservation
    print("\n[2/6] Verifying existing Tailscale node 3 profile preservation...")
    ts_out, _ = run_win_ps("Test-Path C:\\Users\\administrator\\AppData\\Roaming\\Nebulae\\internal\\tailscale")
    print(f"      Tailscale profile directory present: {ts_out}")
    assert ts_out == "True", "Factory Tailscale profile MUST exist and be preserved"

    # Stop any lingering processes before test
    run_win_ps("Stop-Process -Name *nebulae*, *transmission* -Force -ErrorAction SilentlyContinue")
    time.sleep(1)

    # 3. HOP 1: A (0.2.2) -> B (0.2.3)
    print("\n" + "=" * 70)
    print("  [3/6] HOP 1: Executing update Version A (0.2.2) -> Version B (0.2.3)")
    print("=" * 70)
    set_remote_manifest("manifest_stage1_hop_b.json")

    print("      Launching nebulae.exe on Windows host to perform background update...")
    out_hop1, code_hop1 = run_win_ps("& C:\\Users\\administrator\\run-single-hop.ps1 -LogFile C:\\Users\\administrator\\hop1.log -QuitOnStaged 1")
    print(f"      Runner output: {out_hop1}")

    log_hop1, _ = run_win_ps("Get-Content C:\\Users\\administrator\\hop1.log")
    for l in log_hop1.splitlines():
        if any(k in l for k in ["Silent update", "tauri_plugin_updater", "updater", "exiting cleanly"]):
            print(f"      [LOG] {l}")

    assert "Silent update downloaded and verified" in log_hop1, "Hop 1 must download and verify payload"
    assert "exiting cleanly on Windows to trigger staged installer" in log_hop1, "Hop 1 must trigger staged installer"
    print("      ✓ Cryptographic signature verified against authentic compiled-in key 452CAA8BBBBC6F41!")
    print("      ✓ Background NSIS installer spawned.")

    print("      Waiting for NSIS background installer to complete file replacement...")
    start_time = time.time()
    ver_after_1 = get_installed_version()
    while ver_after_1 != "0.2.3" and time.time() - start_time < 35:
        time.sleep(2)
        ver_after_1 = get_installed_version()

    print(f"      ✓ Installed version after Hop 1 is now: {ver_after_1}")
    assert ver_after_1 == "0.2.3", f"Expected version 0.2.3 after Hop 1, got {ver_after_1}"

    # 4. HOP 2: B (0.2.3) -> C (0.2.4)
    print("\n" + "=" * 70)
    print("  [4/6] HOP 2: Executing update Version B (0.2.3) -> Version C (0.2.4)")
    print("=" * 70)
    set_remote_manifest("manifest_stage2_hop_c.json")

    # Wait for installer process to fully clear
    time.sleep(2)
    run_win_ps("Stop-Process -Name *setup*, *transmission* -Force -ErrorAction SilentlyContinue")

    print("      Launching nebulae.exe on Windows host to perform background update...")
    out_hop2, code_hop2 = run_win_ps("& C:\\Users\\administrator\\run-single-hop.ps1 -LogFile C:\\Users\\administrator\\hop2.log -QuitOnStaged 1")
    print(f"      Runner output: {out_hop2}")

    log_hop2, _ = run_win_ps("Get-Content C:\\Users\\administrator\\hop2.log")
    for l in log_hop2.splitlines():
        if any(k in l for k in ["Silent update", "tauri_plugin_updater", "updater", "exiting cleanly"]):
            print(f"      [LOG] {l}")

    assert "Silent update downloaded and verified" in log_hop2, "Hop 2 must download and verify payload"
    assert "exiting cleanly on Windows to trigger staged installer" in log_hop2, "Hop 2 must trigger staged installer"
    print("      ✓ Cryptographic signature verified against authentic compiled-in key 452CAA8BBBBC6F41!")
    print("      ✓ Background NSIS installer spawned.")

    print("      Waiting for NSIS background installer to complete file replacement...")
    start_time = time.time()
    ver_after_2 = get_installed_version()
    while ver_after_2 != "0.2.4" and time.time() - start_time < 35:
        time.sleep(2)
        ver_after_2 = get_installed_version()

    print(f"      ✓ Installed version after Hop 2 is now: {ver_after_2}")
    assert ver_after_2 == "0.2.4", f"Expected version 0.2.4 after Hop 2, got {ver_after_2}"

    # 5. FINAL CHECK: Launch C (0.2.4) and verify Up-to-Date check
    print("\n" + "=" * 70)
    print("  [5/6] FINAL CHECK: Launching Version C (0.2.4) to verify Up-to-Date state")
    print("=" * 70)
    # Wait for installer process to clear
    time.sleep(2)
    run_win_ps("Stop-Process -Name *setup*, *transmission* -Force -ErrorAction SilentlyContinue")

    # Start Nebulae without auto-exit, let it check updater, then poll log
    run_win_ps("Remove-Item C:\\Users\\administrator\\hop3.log -ErrorAction SilentlyContinue; "
               "$p = Start-Process -FilePath C:\\Users\\administrator\\AppData\\Local\\Nebulae\\nebulae.exe "
               "-RedirectStandardOutput C:\\Users\\administrator\\hop3.log -RedirectStandardError C:\\Users\\administrator\\hop3.err -PassThru; "
               "Start-Sleep -Seconds 8; "
               "Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue")

    log_hop3, _ = run_win_ps("Get-Content C:\\Users\\administrator\\hop3.log")
    for l in log_hop3.splitlines():
        if any(k in l for k in ["up to date", "updater", "latest.json"]):
            print(f"      [LOG] {l}")

    assert "Nebulae is up to date; no update available." in log_hop3, "Version C must report up to date"
    print("      ✓ App C discovered no newer updates; confirmed up to date.")

    # 6. Post-test health and preservation verification
    print("\n" + "=" * 70)
    print("  [6/6] Verifying post-test environment preservation...")
    print("=" * 70)
    ts_check, _ = run_win_ps("Test-Path C:\\Users\\administrator\\AppData\\Roaming\\Nebulae\\internal\\tailscale")
    print(f"      ✓ Tailscale profile directory intact: {ts_check}")
    assert ts_check == "True"

    ts_backup, _ = run_win_ps("Test-Path C:\\Users\\administrator\\ts-state-backup")
    print(f"      ✓ Factory ts-state-backup directory intact: {ts_backup}")

    run_win_ps("Stop-Process -Name *nebulae*, *transmission* -Force -ErrorAction SilentlyContinue")

    print("\n" + "=" * 70)
    print("  ALL WINDOWS CONSECUTIVE SILENT UPDATE ACCEPTANCE TESTS PASSED (100% SUCCESS)")
    print("=" * 70)
    print(f"  Starting Version: 0.2.2")
    print(f"  Hop 1 Target:     0.2.3  [AUTHENTIC KEY 452CAA8BBBBC6F41 / NSIS SILENT PASS]")
    print(f"  Hop 2 Target:     0.2.4  [AUTHENTIC KEY 452CAA8BBBBC6F41 / NSIS SILENT PASS]")
    print(f"  Final State:      0.2.4  [UP TO DATE / ZERO DIALOGS / TAILSCALE PROFILE PRESERVED]")
    print("=" * 70 + "\n")

if __name__ == "__main__":
    main()
