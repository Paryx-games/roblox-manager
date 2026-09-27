!macro NSIS_HOOK_PREINSTALL
  Push $0
  Push $1
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RM" "Publisher"
  ReadRegStr $1 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\RM" "UninstallString"
  ${If} $0 == "${MANUFACTURER}"
  ${AndIf} $1 != ""
    DetailPrint "The earlier RM installation must be removed before installing ${PRODUCTNAME}."
    ${IfNot} ${Silent}
      MessageBox MB_OK|MB_ICONSTOP "An earlier installation named RM is still installed. Uninstall RM through Windows Settings, then run this installer again. Saved accounts, settings and presets will be kept."
    ${EndIf}
    Pop $1
    Pop $0
    SetErrorLevel 3
    Abort
  ${EndIf}
  ReadRegStr $0 SHCTX "${UNINSTKEY}" "DisplayVersion"
  ${If} $0 != ""
    nsis_tauri_utils::SemverCompare "${VERSION}" "$0"
    Pop $1
    ${If} $1 = -1
      DetailPrint "A newer version of ${PRODUCTNAME} is installed. Downgrades are blocked."
      ${IfNot} ${Silent}
        MessageBox MB_OK|MB_ICONSTOP "A newer version of ${PRODUCTNAME} is installed. Uninstall it explicitly before installing an older version. Saved accounts, settings and presets will be kept."
      ${EndIf}
      Pop $1
      Pop $0
      SetErrorLevel 2
      Abort
    ${EndIf}
  ${EndIf}
  Pop $1
  Pop $0
  DetailPrint "${PRODUCTNAME} updates preserve saved accounts, settings and presets."
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    DetailPrint "Saved accounts, settings and presets remain where stored (normally $APPDATA\RM)."
    ${IfNot} ${Silent}
    ${AndIf} $PassiveMode <> 1
      MessageBox MB_OK|MB_ICONINFORMATION "Uninstalling ${PRODUCTNAME} keeps your saved accounts, settings and presets where they are stored (normally $APPDATA\RM).$\r$\n$\r$\nThe browser-data checkbox only removes ${PRODUCTNAME}'s interface browser data. Microsoft WebView2 Runtime is shared with other apps and will remain installed."
    ${EndIf}
  ${EndIf}
!macroend
