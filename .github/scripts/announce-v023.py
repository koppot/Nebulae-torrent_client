"""One owner-authorized v0.2.3 announcement; dry-run unless --send is supplied."""

import json
import os
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request

REPO = "koppot/Nebulae-torrent_client"
TAG = "v0.2.3"
SOURCE = "809aef1b5cc1189ef64d8695c8dad7559e1141cd"
BASE = f"https://github.com/{REPO}"
RELEASE = f"{BASE}/releases/tag/{TAG}"
DOWNLOAD = f"{BASE}/releases/download/{TAG}"
PACKAGES = (
    "Nebulae-Setup-x64.exe",
    "nebulae.exe",
    "Nebulae-macOS-arm64.dmg",
    "Nebulae-macOS-x64.dmg",
    "Nebulae-Linux-x86_64.AppImage",
    "Nebulae-Linux-x86_64.deb",
    "Nebulae-Linux-x86_64.rpm",
    "SHA256SUMS.txt",
)


def gh_json(*args):
    return json.loads(subprocess.check_output(["gh", *args], text=True))


def validate_release():
    release = gh_json(
        "release", "view", TAG, "--repo", REPO,
        "--json", "isDraft,isPrerelease,url,assets",
    )
    if release["isDraft"] or release["isPrerelease"] or release["url"] != RELEASE:
        raise SystemExit("Expected the published stable v0.2.3 release.")
    commit = gh_json("api", f"repos/{REPO}/commits/{TAG}")
    if commit["sha"] != SOURCE:
        raise SystemExit("Release source differs from the approved v0.2.3 source.")
    assets = {asset["name"]: asset for asset in release["assets"]}
    for name in PACKAGES:
        asset = assets.get(name)
        if (
            not asset or asset["size"] <= 0 or asset["state"] != "uploaded"
            or asset["url"] != f"{DOWNLOAD}/{name}"
        ):
            raise SystemExit(f"Required release asset unavailable: {name}")


def link(label, name):
    return f"[{label}]({DOWNLOAD}/{name})"


def make_payload():
    embed = {
        "title": "Nebulae 0.2.3 is here",
        "url": RELEASE,
        "description": (
            "**Your torrents. Your library. One native app.**\n\n"
            "The latest **Nebulae** release is now available for "
            "**Windows, macOS and Linux**.\n\n"
            "Built for reliable downloading and long-term seeding, with "
            "an embedded VLC player for your media library.\n\n"
            "**In this release**\n"
            "• Latest recovery-branch application updates\n"
            "• Updated disc navigation and DVD chapter-mark handling\n"
            "• Bundled playback runtime, with no separate VLC installation"
        ),
        "color": 15010068,
        "thumbnail": {"url": "https://asiandvdclub.org/images/logo.png"},
        "fields": [
            {
                "name": "Windows · x64",
                "value": (
                    link("Download installer", "Nebulae-Setup-x64.exe")
                    + " · " + link("Executable", "nebulae.exe")
                ),
                "inline": False,
            },
            {
                "name": "macOS · choose your Mac",
                "value": (
                    link("Apple Silicon DMG", "Nebulae-macOS-arm64.dmg")
                    + " · " + link("Intel DMG", "Nebulae-macOS-x64.dmg")
                ),
                "inline": False,
            },
            {
                "name": "Linux · x86_64",
                "value": (
                    link("AppImage", "Nebulae-Linux-x86_64.AppImage")
                    + " · " + link("DEB", "Nebulae-Linux-x86_64.deb")
                    + " · " + link("RPM", "Nebulae-Linux-x86_64.rpm")
                ),
                "inline": False,
            },
            {
                "name": "Before installing",
                "value": (
                    "Windows builds are unsigned; SmartScreen may warn. "
                    "Blu-ray/UHD menu startup and HDR color have known "
                    "reported limitations; playback corrections are not "
                    "user-confirmed. Encrypted discs require prior decryption."
                ),
                "inline": False,
            },
            {
                "name": "Get started · share your feedback",
                "value": (
                    f"[Full release & all downloads]({RELEASE})"
                    + " · " + link("SHA-256 checksums", "SHA256SUMS.txt")
                    + f" · [Report an issue]({BASE}/issues)\n\n"
                    "Try the new release and tell us how it works on your setup."
                ),
                "inline": False,
            },
        ],
        "footer": {"text": "Asian DVD Club · Nebulae 0.2.3"},
    }
    return {
        "content": "@everyone",
        "embeds": [embed],
        # The owner explicitly authorized one @everyone notification.
        "allowed_mentions": {"parse": ["everyone"]},
    }


def validate_payload(payload):
    assert payload["content"] == "@everyone"
    assert payload["allowed_mentions"] == {"parse": ["everyone"]}
    assert len(payload["embeds"]) == 1
    embed = payload["embeds"][0]
    assert len(embed["title"]) <= 256
    assert len(embed["description"]) <= 4096
    assert len(embed["fields"]) <= 25
    for field in embed["fields"]:
        assert len(field["name"]) <= 256
        assert 0 < len(field["value"]) <= 1024
    text_size = len(embed["title"]) + len(embed["description"])
    text_size += len(embed["footer"]["text"])
    text_size += sum(len(f["name"]) + len(f["value"]) for f in embed["fields"])
    assert text_size <= 6000


def discord_json(url, payload=None):
    body = None if payload is None else json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        url, data=body,
        headers={"Content-Type": "application/json", "User-Agent": "Nebulae-Announcement"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        raise SystemExit(f"Discord returned HTTP {error.code}; no automatic retry.") from None
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError):
        # Never print an exception that could contain the secret webhook URL.
        raise SystemExit(
            "Discord response unavailable; delivery may be uncertain. "
            "Do not resend without checking the channel."
        ) from None


def send_once(payload):
    raw_url = os.environ.get("WEBHOOK_URL", "")
    parsed = urllib.parse.urlsplit(raw_url)
    if (
        parsed.scheme != "https"
        or parsed.hostname not in ("discord.com", "discordapp.com")
        or not parsed.path.startswith("/api/webhooks/")
        or parsed.username or parsed.password or parsed.query or parsed.fragment
    ):
        raise SystemExit("The existing Discord webhook is absent or invalid.")
    webhook_url = urllib.parse.urlunsplit(parsed)
    metadata = discord_json(webhook_url)
    if not metadata.get("guild_id") or not metadata.get("channel_id"):
        raise SystemExit("Expected the established server-channel webhook.")
    # One POST only. Do not retry an ambiguous response and risk another ping.
    message = discord_json(webhook_url + "?wait=true", payload)
    message_id = message.get("id")
    channel_id = message.get("channel_id")
    if not message_id or channel_id != metadata["channel_id"]:
        raise SystemExit("Discord response lacks the expected message identity; do not resend.")
    jump = f"https://discord.com/channels/{metadata['guild_id']}/{channel_id}/{message_id}"
    print(f"Discord accepted announcement: {jump}", flush=True)
    posted = discord_json(webhook_url + f"/messages/{message_id}")
    if (
        posted.get("content") != payload["content"]
        or len(posted.get("embeds", [])) != 1
        or any(
            posted["embeds"][0].get(key) != payload["embeds"][0][key]
            for key in ("title", "description", "fields")
        )
    ):
        raise SystemExit("Message was sent but read-back verification differs; do not resend.")
    print(f"Verified Discord announcement: {jump}")
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as output:
            output.write(f"## Nebulae 0.2.3 announcement\n\n[View Discord message]({jump})\n")
            output.write(f"\nSource: `{SOURCE}`. One authorized `@everyone` mention.\n")


if __name__ == "__main__":
    if sys.argv[1:] not in ([], ["--send"]):
        raise SystemExit("Usage: announce-v023.py [--send]")
    validate_release()
    payload = make_payload()
    validate_payload(payload)
    if sys.argv[1:] == ["--send"]:
        send_once(payload)
    else:
        print(json.dumps(payload, indent=2, ensure_ascii=False))
