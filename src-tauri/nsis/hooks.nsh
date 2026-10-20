!macro NSIS_HOOK_PREINSTALL
  ; Upgrade safety: stop THIS installation's app and its OWNED sidecars
  ; (matched by executable path under $INSTDIR) before replacing files.
  ; Bounded graceful-then-forced shutdown; unrelated same-name processes
  ; are untouched. On failure: actionable message and abort BEFORE
  ; extraction, so a failed upgrade never leaves a mixed-version install.
  ; nsExec keeps the helper command silent (no console window).
  nsExec::Exec `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$ErrorActionPreference='SilentlyContinue'; $$dir='$INSTDIR'; Get-Process nebulae -ErrorAction SilentlyContinue | Where-Object { $$_.Path -eq ($$dir + '\nebulae.exe') } | ForEach-Object { [void]$$_.CloseMainWindow() }; Start-Sleep -Seconds 3; Get-Process nebulae,transmission-daemon,tailscaled -ErrorAction SilentlyContinue | Where-Object { $$_.Path -like ($$dir + '*') } | ForEach-Object { Stop-Process -Id $$_.Id -Force -ErrorAction SilentlyContinue }; Start-Sleep -Seconds 2; if (Get-Process nebulae,transmission-daemon,tailscaled -ErrorAction SilentlyContinue | Where-Object { $$_.Path -like ($$dir + '*') }) { exit 1 }; exit 0"`
  Pop $0
  IntCmp $0 0 preinstall_shutdown_ok
  MessageBox MB_OK|MB_ICONEXCLAMATION "Nebulae could not be stopped for the update.$\n$\nPlease close Nebulae (check Task Manager for nebulae.exe, transmission-daemon.exe or tailscaled.exe started from:$\n$INSTDIR) and run this installer again."
  Abort
preinstall_shutdown_ok:
  DetailPrint "Stopped Nebulae and its owned background services for upgrade."
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

  ; SignPath Foundation auto-update disclosure:
  ; "Applications that update themselves silently must allow users to disable this behavior.
  ; A notification must be displayed to the user that an update will be applied, e.g. in the installer."
  DetailPrint "Automatic Updates: Nebulae keeps itself updated silently in the background."
  DetailPrint "To disable this, turn off 'Automatically keep Nebulae updated' in Settings -> Behavior."
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Clean up protocol handlers
  DeleteRegKey HKCU "Software\Classes\magnet"
  DeleteRegKey HKCU "Software\Classes\adc"

  ; Clean up file associations
  DeleteRegKey HKCU "Software\Classes\Nebulae.torrent"
  DeleteRegKey HKCU "Software\Classes\Nebulae.adc"
  DeleteRegValue HKCU "Software\Classes\.torrent" ""
  DeleteRegValue HKCU "Software\Classes\.adc" ""

  ; Refresh shell icon cache
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
