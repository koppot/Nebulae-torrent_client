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

    sig_mac_arm = read_sig("artifacts/**/Nebulae-macOS-arm64*tar.gz.sig") or read_sig("artifacts/**/*arm64*.sig") or read_sig("/tmp/nebulae-sigs/*arm64*.sig")
    if sig_mac_arm:
        manifest["platforms"]["darwin-aarch64"] = {
            "signature": sig_mac_arm,
            "url": f"{base_url}/Nebulae-macOS-arm64.dmg-app.tar.gz"
        }

    sig_mac_x64 = read_sig("artifacts/**/Nebulae-macOS-x64*tar.gz.sig") or read_sig("artifacts/**/*x64*darwin*.sig") or read_sig("/tmp/nebulae-sigs/*macOS-x64*.sig")
    if sig_mac_x64:
        manifest["platforms"]["darwin-x86_64"] = {
            "signature": sig_mac_x64,
            "url": f"{base_url}/Nebulae-macOS-x64.dmg-app.tar.gz"
        }

    sig_linux = read_sig("artifacts/**/Nebulae-Linux*AppImage.tar.gz.sig") or read_sig("artifacts/**/*linux*.sig")
    if sig_linux:
        manifest["platforms"]["linux-x86_64"] = {
            "signature": sig_linux,
            "url": f"{base_url}/Nebulae-Linux-x86_64.AppImage.tar.gz"
        }

    sig_win = read_sig("artifacts/**/Nebulae-Setup-x64.exe.sig") or read_sig("artifacts/**/*setup*.sig") or read_sig("/tmp/nebulae-sigs/*Setup*.sig")
    if sig_win:
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
