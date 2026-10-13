!macro NSIS_HOOK_PREINSTALL
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
