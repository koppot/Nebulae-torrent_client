# VLC 3.0.23 Runtime Patches

## Source Identity
- **Version**: VLC 3.0.23 "Vetinari"
- **SHA-256 of patched source file**:
  - `modules/codec/videotoolbox.m`: 5c6a7e87c96fb7902424f7f6918f4adcf987b734f616a455be7815a928af77f6

## Patches Applied

### vt-305.patch
**Topic**: VideoToolbox session restart optimization

**Rationale**: During 1080p SDR → 4K HDR format transitions, the VideoToolbox decoder returns OSStatus 305 (`kVTVideoDecoderBadDataErr`). The current VLC 3.0.23 code invalidates the session unconditionally, then recreates it. The format description from the previous session is not released, so the new session creation may fail or trigger BadDataErr.

**Changes**:
1. Before creating a new format description in `StartVideoToolbox()`, check if the existing session can accept the new format via `VTDecompressionSessionCanAcceptFormatDescription()`. If yes, reuse the session.
2. In `StopVideoToolbox()`, release `videoFormatDescription` after invalidation so it can be recreated with the new format on next `StartVideoToolbox()`.
3. The pause/unpause workaround in `return_to_menu` remains as a legacy fallback (user-space buffer drain via the pause pipeline).

**Not Verified**: This patch has NOT been compiled or tested. It is a candidate changeset based on MacLC 1.3's approach of using `CanAcceptFormatDescription` to avoid unnecessary session invalidation.

## Build Requirements
- bison 3.0+ (system has 2.3, requires fix)
- C++11-capable compiler (system C++ headers misconfigured)
- Standard autotools (autoconf, automake)

## Runtime Integration
After building the patched VLC 3.0.23, copy:
- `lib/libvlc.5.dylib` → `src-tauri/resources/vlc/lib/libvlc.5.dylib`
- `lib/libvlccore.9.dylib` → `src-tauri/resources/vlc/lib/libvlccore.9.dylib`
- `plugins/*.dylib` → `src-tauri/resources/vlc/plugins/`

## Notes
- MacLC 1.3 uses `pic_pacer` (VLC 4) which is not backportable to VLC 3.0.23
- The pause/unpause workaround is retained as user-space mitigation
- Objective-C bindings for display notifications not included here
- HDR rendering requires caopengllayer changes beyond this patch
