; Dynamic Island installer hooks (NSIS).
; Before uninstalling, undo what the app set up outside its own folder:
; - the Claude Code hooks in ~/.claude/settings.json (else they'd point at a deleted island-hook.exe)
; - the "start with Windows" entry (else Windows would try to launch a deleted exe)
!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\dynamic-island.exe" hooks uninstall'
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Dynamic Island"
!macroend
