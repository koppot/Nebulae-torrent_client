export interface SpeedDisplayControl {
  label: string;
  help: string;
}

type Platform = 'windows' | 'macos' | 'linux' | 'unknown';

/**
 * The speed readout the setting controls differs per OS, so the control is labeled for the
 * surface the user can actually see: the Windows taskbar band or the macOS menu bar.
 * Returns null where no such readout exists (Linux), so no dead control is shown.
 */
export function speedDisplayControl(platform: Platform, userAgent = ''): SpeedDisplayControl | null {
  const resolved: Platform =
    platform !== 'unknown'
      ? platform
      : /windows/i.test(userAgent)
        ? 'windows'
        : /macintosh|mac os x/i.test(userAgent)
          ? 'macos'
          : /linux/i.test(userAgent)
            ? 'linux'
            : 'unknown';
  if (resolved === 'windows' || resolved === 'unknown') {
    return {
      label: 'Show taskbar upload/download speeds',
      help: 'Shows live upload and download speeds next to the notification area on the Windows taskbar. Turning it off hides the overlay immediately and keeps it hidden, including in fullscreen and after restarts. Torrent activity and the tray icon are not affected.',
    };
  }
  if (resolved === 'macos') {
    return {
      label: 'Show menu bar upload/download speeds',
      help: 'Shows live upload and download speeds in the Nebulae menu bar icon. Turning it off restores the plain icon immediately and keeps it that way after restarts.',
    };
  }
  return null;
}
