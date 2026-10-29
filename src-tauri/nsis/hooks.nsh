; Nebulae installer hooks (tauri-bundler 2.10.x NSIS template, installerHooks).
;
; What these hooks deliberately do NOT do, and why:
; - no taskkill by image name: `taskkill /IM transmission-daemon.exe` would
;   also kill a Transmission that has nothing to do with Nebulae. Sidecars
;   are stopped through the Restart Manager scoped to the files under
;   $INSTDIR, and the main app is handled by the template's own
;   CheckIfAppIsRunning (Restart Manager on $INSTDIR\nebulae.exe).
; - no .torrent/.adc association writes: the bundler's fileAssociations
;   config registers and unregisters them (APP_ASSOCIATE/APP_UNASSOCIATE).
; - no registry or firewall cleanup during updates: the updater runs the
;   uninstaller with /UPDATE, and $UpdateMode = 1 must leave handlers,
;   rules and data untouched.

; StrFunc generates each helper as two functions: StrLoc for install context
; and un.StrLoc for uninstall context. The template invokes ${StrLoc} at
; global scope, which only generates the install variant; the firewall-rule
; macro below runs in the uninstall section, so the uninstall variant must be
; generated here at include scope (first invocation defines the function).
${UnStrLoc}

; Stop one sidecar gracefully, only if a running process holds the file.
; Uses the same Restart Manager pattern as the template's CheckIfAppIsRunning,
; without the prompt (sidecars have no UI to answer one).
!macro _NEBULAE_RM_SHUTDOWN FILEPATH
  ${If} ${FileExists} "${FILEPATH}"
    !insertmacro RestartManager_StartSession $R0
    ${If} $R0 != ""
      !insertmacro RestartManager_RegisterFile $R0 "${FILEPATH}"
      ${If} $0 = 0
        System::Call 'RSTRTMGR::RmGetList(p R0, *i .r1, *i .r2, p 0, *i .r3) i .r0'
        ; ERROR_SUCCESS means nothing holds the file; ERROR_MORE_DATA lists users.
        ${If} $0 = ${ERROR_MORE_DATA}
          DetailPrint "Stopping process using ${FILEPATH}"
          System::Call 'RSTRTMGR::RmShutdown(p R0, i ${RmForceShutdown}, p 0) i .r0'
          ${If} $0 <> 0
            DetailPrint "Could not stop the process using ${FILEPATH} (RmShutdown: $0)"
          ${EndIf}
        ${EndIf}
      ${EndIf}
      !insertmacro RestartManager_EndSession $R0
    ${EndIf}
  ${EndIf}
!macroend

!macro _NEBULAE_STOP_OWNED_SIDECARS
  !insertmacro _NEBULAE_RM_SHUTDOWN "$INSTDIR\resources\engine\transmission\transmission-daemon.exe"
  !insertmacro _NEBULAE_RM_SHUTDOWN "$INSTDIR\resources\engine\tailscale\tailscaled.exe"
!macroend

; Add an inbound firewall rule for one of our executables. Needs elevation;
; without it the rule write fails and Windows shows its own prompt on first
; inbound connection, which is acceptable and expected for currentUser installs.
!macro _NEBULAE_FWRULE_ADD RULENAME EXEPATH
  UserInfo::GetAccountType
  Pop $R8
  ${If} $R8 == "Admin"
  ${OrIf} $R8 == "Power"
    nsExec::Exec `netsh.exe advfirewall firewall add rule name="${RULENAME}" dir=in action=allow program="${EXEPATH}" enable=yes`
    Pop $R7
    ${If} $R7 <> 0
      DetailPrint "Firewall rule '${RULENAME}' could not be added (netsh: $R7); Windows may ask on first run."
    ${EndIf}
  ${Else}
    DetailPrint "Not elevated: skipping firewall rule '${RULENAME}'; Windows may ask on first run."
  ${EndIf}
!macroend

; Delete a firewall rule only when it exists and its program is ours.
!macro _NEBULAE_FWRULE_DELETE_IF_OURS RULENAME EXEPATH
  UserInfo::GetAccountType
  Pop $R8
  ${If} $R8 == "Admin"
  ${OrIf} $R8 == "Power"
    nsExec::ExecToStack `netsh.exe advfirewall firewall show rule name="${RULENAME}"`
    Pop $R7
    ${If} $R7 == 0
      StrCpy $R6 0
      ${Do}
        Pop $R5
        ${If} $R5 == "error"
          ${ExitDo}
        ${EndIf}
        ${UnStrLoc} $R4 $R5 "${EXEPATH}" ">"
        ${If} $R4 != ""
          StrCpy $R6 1
        ${EndIf}
      ${Loop}
      ${If} $R6 == 1
        nsExec::Exec `netsh.exe advfirewall firewall delete rule name="${RULENAME}"`
        DetailPrint "Removed firewall rule '${RULENAME}'."
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !if "${ARCH}" == "x64"
    ${IfNot} ${RunningX64}
      MessageBox MB_ICONSTOP "Nebulae requires a 64-bit version of Windows."
      Abort
    ${EndIf}
  !endif
  ; Free locked sidecar files before the template replaces them; the template
  ; then handles nebulae.exe itself with a user prompt.
  !insertmacro _NEBULAE_STOP_OWNED_SIDECARS
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; magnet: URL scheme (deep links are not part of the bundler's config here)
  WriteRegStr HKCU "Software\Classes\magnet" "" "URL:BitTorrent Magnet Link"
  WriteRegStr HKCU "Software\Classes\magnet" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\magnet\DefaultIcon" "" "$INSTDIR\nebulae.exe,0"
  WriteRegStr HKCU "Software\Classes\magnet\shell\open\command" "" '"$INSTDIR\nebulae.exe" "%1"'

  ; adc: URL scheme
  WriteRegStr HKCU "Software\Classes\adc" "" "URL:Asian DVD Club Link"
  WriteRegStr HKCU "Software\Classes\adc" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\adc\DefaultIcon" "" "$INSTDIR\nebulae.exe,0"
  WriteRegStr HKCU "Software\Classes\adc\shell\open\command" "" '"$INSTDIR\nebulae.exe" "%1"'

  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'

  !insertmacro _NEBULAE_FWRULE_ADD "Nebulae Torrent Client" "$INSTDIR\nebulae.exe"
  !insertmacro _NEBULAE_FWRULE_ADD "Nebulae Transmission Daemon" "$INSTDIR\resources\engine\transmission\transmission-daemon.exe"

  ; SignPath Foundation auto-update disclosure:
  ; "Applications that update themselves silently must allow users to disable this behavior.
  ; A notification must be displayed to the user that an update will be applied, e.g. in the installer."
  DetailPrint "Automatic Updates: Nebulae keeps itself updated silently in the background."
  DetailPrint "To disable this, turn off 'Automatically keep Nebulae updated' in Settings -> Behavior."
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro _NEBULAE_STOP_OWNED_SIDECARS

  ; Updates run this uninstaller with /UPDATE: keep everything registered then.
  ${If} $UpdateMode <> 1
    ; Remove the protocol handlers only when they still point at this install.
    ReadRegStr $R7 HKCU "Software\Classes\magnet\shell\open\command" ""
    ${If} $R7 == '"$INSTDIR\nebulae.exe" "%1"'
      DeleteRegKey HKCU "Software\Classes\magnet"
    ${EndIf}
    ReadRegStr $R7 HKCU "Software\Classes\adc\shell\open\command" ""
    ${If} $R7 == '"$INSTDIR\nebulae.exe" "%1"'
      DeleteRegKey HKCU "Software\Classes\adc"
    ${EndIf}

    !insertmacro _NEBULAE_FWRULE_DELETE_IF_OURS "Nebulae Torrent Client" "$INSTDIR\nebulae.exe"
    !insertmacro _NEBULAE_FWRULE_DELETE_IF_OURS "Nebulae Transmission Daemon" "$INSTDIR\resources\engine\transmission\transmission-daemon.exe"

    System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; The template's "delete app data" checkbox targets the bundle id
  ; ($APPDATA\org.asiandvdclub.nebulae); the app's real data lives in
  ; %APPDATA%\Nebulae (and %LOCALAPPDATA%\Nebulae for local-only data).
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RmDir /r "$APPDATA\Nebulae"
    RmDir /r "$LOCALAPPDATA\Nebulae"
    DetailPrint "Removed Nebulae application data."
  ${EndIf}
!macroend
