!macro NSIS_HOOK_PREINSTALL
  ; Upgrade safety: directly terminate any running nebulae or owned sidecars
  ; without any CMD, PowerShell, batch-file, or shell script wrappers.
  ; nsExec::Exec executes binaries directly with CREATE_NO_WINDOW, ensuring zero visible console windows.
  nsExec::Exec `taskkill.exe /F /T /IM nebulae.exe`
  nsExec::Exec `taskkill.exe /F /T /IM transmission-daemon.exe`
  nsExec::Exec `taskkill.exe /F /T /IM tailscaled.exe`
  DetailPrint "Stopped Nebulae and background services for upgrade."
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Register magnet: URL scheme handler in Windows Registry
  WriteRegStr HKCU "Software\Classes\magnet" "" "URL:BitTorrent Magnet Link"
  WriteRegStr HKCU "Software\Classes\magnet" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\magnet\DefaultIcon" "" "$INSTDIR\nebulae.exe,0"
  WriteRegStr HKCU "Software\Classes\magnet\shell\open\command" "" '"$INSTDIR\nebulae.exe" "%1"'

  ; Register adc: URL scheme handler in Windows Registry
  WriteRegStr HKCU "Software\Classes\adc" "" "URL:Asian DVD Club Link"
  WriteRegStr HKCU "Software\Classes\adc" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\adc\DefaultIcon" "" "$INSTDIR\nebulae.exe,0"
  WriteRegStr HKCU "Software\Classes\adc\shell\open\command" "" '"$INSTDIR\nebulae.exe" "%1"'

  ; Ensure .torrent file association
  WriteRegStr HKCU "Software\Classes\.torrent" "" "Nebulae.torrent"
  WriteRegStr HKCU "Software\Classes\Nebulae.torrent" "" "BitTorrent Metainfo File"
  WriteRegStr HKCU "Software\Classes\Nebulae.torrent\DefaultIcon" "" "$INSTDIR\nebulae.exe,0"
  WriteRegStr HKCU "Software\Classes\Nebulae.torrent\shell\open\command" "" '"$INSTDIR\nebulae.exe" "%1"'

  ; Ensure .adc file association
  WriteRegStr HKCU "Software\Classes\.adc" "" "Nebulae.adc"
  WriteRegStr HKCU "Software\Classes\Nebulae.adc" "" "Asian DVD Club Metainfo File"
  WriteRegStr HKCU "Software\Classes\Nebulae.adc\DefaultIcon" "" "$INSTDIR\nebulae.exe,0"
  WriteRegStr HKCU "Software\Classes\Nebulae.adc\shell\open\command" "" '"$INSTDIR\nebulae.exe" "%1"'

  ; Refresh shell icon cache
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'

  ; Register inbound firewall rules for Nebulae and transmission-daemon silently without visible windows
  nsExec::Exec `netsh.exe advfirewall firewall add rule name="Nebulae Torrent Client" dir=in action=allow program="$INSTDIR\nebulae.exe" enable=yes`
  nsExec::Exec `netsh.exe advfirewall firewall add rule name="Nebulae Transmission Daemon" dir=in action=allow program="$INSTDIR\resources\engine\transmission\transmission-daemon.exe" enable=yes`

  ; SignPath Foundation auto-update disclosure:
  ; "Applications that update themselves silently must allow users to disable this behavior.
  ; A notification must be displayed to the user that an update will be applied, e.g. in the installer."
  DetailPrint "Automatic Updates: Nebulae keeps itself updated silently in the background."
  DetailPrint "To disable this, turn off 'Automatically keep Nebulae updated' in Settings -> Behavior."
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Terminate running processes before uninstallation to prevent locked file errors
  nsExec::Exec `taskkill.exe /F /T /IM nebulae.exe`
  nsExec::Exec `taskkill.exe /F /T /IM transmission-daemon.exe`
  nsExec::Exec `taskkill.exe /F /T /IM tailscaled.exe`

  ; Clean up protocol handlers
  DeleteRegKey HKCU "Software\Classes\magnet"
  DeleteRegKey HKCU "Software\Classes\adc"

  ; Clean up file associations
  DeleteRegKey HKCU "Software\Classes\Nebulae.torrent"
  DeleteRegKey HKCU "Software\Classes\Nebulae.adc"
  DeleteRegValue HKCU "Software\Classes\.torrent" ""
  DeleteRegValue HKCU "Software\Classes\.adc" ""

  ; Remove firewall rules
  nsExec::Exec `netsh.exe advfirewall firewall delete rule name="Nebulae Torrent Client"`
  nsExec::Exec `netsh.exe advfirewall firewall delete rule name="Nebulae Transmission Daemon"`

  ; Refresh shell icon cache
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
