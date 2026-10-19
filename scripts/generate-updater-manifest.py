#!/usr/bin/env python3
"""
Generate a Tauri 2-compliant latest.json updater manifest for draft test-builds release.
"""
import os
import glob
import json
import datetime
from pathlib import Path

def main():
    repo = os.environ.get("GH_REPO", "koppot/Nebulae-torrent_client").strip()
    repo_url = f"https://github.com/{repo}"
    base_url = f"{repo_url}/releases/download/test-builds"

    # Read version from tauri.conf.json
    conf_path = Path("src-tauri/tauri.conf.json")
    if conf_path.exists():
        with open(conf_path, "r", encoding="utf-8") as f:
            version = json.load(f).get("version", "0.2.2")
    else:
        version = os.environ.get("APP_VERSION", "0.2.2").strip()

    def read_sig(pattern):
        matches = glob.glob(pattern, recursive=True)
        if matches:
            with open(matches[0], "r", encoding="utf-8") as f:
                return f.read().strip()
        return ""

    manifest = {
        "version": version,
        "notes": f"Nebulae test build ({version}) with silent background auto-updates.",
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": {}
    }

    import base64

    def verify_sig_comment(sig_str, expected_needle, platform_name):
        if not sig_str:
            return
        decoded_text = sig_str
        if not sig_str.startswith("untrusted comment"):
            try:
                decoded_text = base64.b64decode(sig_str.strip()).decode("utf-8", errors="ignore")
            except Exception as e:
                raise RuntimeError(f"FATAL: Could not base64 decode {platform_name} signature: {e}")
        
        # Verify trusted comment exists and contains expected needle
        trusted_line = ""
        for line in decoded_text.splitlines():
            if line.startswith("trusted comment:"):
                trusted_line = line
                break
        
        if not trusted_line:
            raise RuntimeError(f"FATAL: Missing trusted comment in {platform_name} signature!")
        
        if expected_needle.lower() not in trusted_line.lower():
            raise RuntimeError(
                f"FATAL: {platform_name} signature trusted comment ({trusted_line}) "
                f"does not match expected file needle '{expected_needle}'! Aborting publication."
            )
        print(f"Verified {platform_name} signature: {trusted_line}")

    sig_mac_arm = read_sig("artifacts/**/Nebulae-macOS-arm64*tar.gz.sig")
    if sig_mac_arm:
        verify_sig_comment(sig_mac_arm, "Nebulae.app.tar.gz", "darwin-aarch64")
        manifest["platforms"]["darwin-aarch64"] = {
            "signature": sig_mac_arm,
            "url": f"{base_url}/Nebulae-macOS-arm64.dmg-app.tar.gz"
        }

    sig_mac_x64 = read_sig("artifacts/**/Nebulae-macOS-x64*tar.gz.sig")
    if sig_mac_x64:
        verify_sig_comment(sig_mac_x64, "Nebulae.app.tar.gz", "darwin-x86_64")
        manifest["platforms"]["darwin-x86_64"] = {
            "signature": sig_mac_x64,
            "url": f"{base_url}/Nebulae-macOS-x64.dmg-app.tar.gz"
        }

    sig_linux = read_sig("artifacts/**/Nebulae-Linux*AppImage.tar.gz.sig")
    if sig_linux:
        verify_sig_comment(sig_linux, "AppImage", "linux-x86_64")
        manifest["platforms"]["linux-x86_64"] = {
            "signature": sig_linux,
            "url": f"{base_url}/Nebulae-Linux-x86_64.AppImage.tar.gz"
        }

    sig_win = read_sig("artifacts/**/Nebulae-Setup-x64.exe.sig")
    if sig_win:
        verify_sig_comment(sig_win, "setup.exe", "windows-x86_64")
        manifest["platforms"]["windows-x86_64"] = {
            "signature": sig_win,
            "url": f"{base_url}/Nebulae-Setup-x64.exe"
        }

    out_file = Path("latest.json")
    with open(out_file, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)

    print("Generated test-builds latest.json:")
    print(json.dumps(manifest, indent=2))

if __name__ == "__main__":
    main()
