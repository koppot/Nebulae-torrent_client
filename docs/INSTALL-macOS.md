# Installing Nebulae on macOS

Thank you for installing Nebulae. macOS Gatekeeper checks apps that are not
downloaded from the App Store. Three short steps:

## Step 1: Drag Nebulae to Applications

Drag the Nebulae icon onto the Applications folder icon in the dmg window.

## Step 2: Open it, click Done on the Apple warning

Open Nebulae from Applications. macOS shows a warning that the app cannot be
checked for malicious software. Click **Done** (do NOT click "Move to
Trash").

## Step 3: System Settings > Privacy & Security > Open Anyway

Open System Settings, go to **Privacy & Security**, find the note about
Nebulae being blocked, and click **Open Anyway**. Nebulae opens.

Nebulae opens normally from now on; steps 2 and 3 are only needed the first
time.

## Notes

- If the Open Anyway button is missing, open Nebulae once more; macOS only
  shows the entry after a blocked launch attempt.
- Nothing is installed outside the Applications folder. To remove Nebulae,
  drag it from Applications to the Trash; your data stays in
  `~/Library/Application Support/Nebulae`.
- After installing an update, macOS may ask again for folder permissions
  (for example, removable-volume access). This is expected for
  ad-hoc-signed builds: macOS keys saved permission grants to the app's
  code-signing identity, and an ad-hoc signature changes with every build.
  Re-grant the permission once per update; no data is affected.

The same text ships inside every macOS dmg as `READ ME FIRST.txt`
(source: `docs/macos-install-readme.txt`).
