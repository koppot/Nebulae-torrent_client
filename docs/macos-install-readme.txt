INSTALLING NEBULAE ON MAC

You may see a warning that Apple cannot check the app for malicious
software, or that the app cannot be opened. This is expected: this build
is not notarized.

1. Open the DMG file.
2. Drag Nebulae.app to the Applications folder.
3. Eject the DMG (Finder sidebar, Eject).
4. Open Applications and double-click Nebulae.
5. If macOS says the app cannot be opened:
   - macOS 15 (Sequoia) or later: click Done, then open System Settings,
     Privacy and Security, scroll down to the Security section, and click
     Open Anyway.
   - macOS 14 (Sonoma) or earlier: control-click (right-click) Nebulae.app
     in Applications and choose Open, then Open in the dialog.
6. If macOS blocks the app because it was downloaded from the internet,
   run this in Terminal and try again:
   xattr -dr com.apple.quarantine /Applications/Nebulae.app
7. If macOS asks about Nebulae reading removable volumes (external
   drives), click Allow.

Release SHA-256 checksums are published on the release page at
nebulae.asiandvdclub.org.
