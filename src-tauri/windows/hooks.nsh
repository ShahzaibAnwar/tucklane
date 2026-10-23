; Tucklane NSIS installer hooks.
; On uninstall, remove the "start with Windows" registration created by
; tauri-plugin-autostart so no dangling startup entry remains. User files in
; the managed Downloads folder are never touched by the installer.

!macro NSIS_HOOK_PREUNINSTALL
  ; Stop a running instance so its files can be removed.
  nsExec::Exec 'taskkill /IM Tucklane.exe /F'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Tucklane"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Tucklane"
!macroend
