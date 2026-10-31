; Pad Companion installer hooks (Tauri NSIS).
; Drivers are never bundled: the installer checks for them and offers the official download pages.

!macro NSIS_HOOK_POSTINSTALL
  ClearErrors
  ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Services\ViGEmBus" "ImagePath"
  ${If} ${Errors}
    MessageBox MB_YESNO|MB_ICONINFORMATION "Pad Companion needs the ViGEmBus driver so games can see the controller as an Xbox 360 pad.$\r$\n$\r$\nOpen the official download page now?" IDNO +2
    ExecShell "open" "https://github.com/nefarius/ViGEmBus/releases/latest"
  ${EndIf}
  ClearErrors
  ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Services\HidHide" "ImagePath"
  ${If} ${Errors}
    MessageBox MB_YESNO|MB_ICONINFORMATION "HidHide is recommended so games and Steam see only one controller (no double input).$\r$\n$\r$\nOpen the official download page now?" IDNO +2
    ExecShell "open" "https://github.com/nefarius/HidHide/releases/latest"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Stop the tray app, then let it un-hide the pad and remove its autostart entry.
  nsExec::Exec 'taskkill /IM pad-companion.exe /F'
  Sleep 500
  nsExec::Exec '"$INSTDIR\pad-companion.exe" --cleanup'
!macroend
