; Dynamic Island installer hooks (NSIS).
; Before uninstalling, undo what the app set up outside its own folder
; (Claude Code hooks in ~/.claude/settings.json, "start with Windows" entry).
; The MSI does the same through windows/msi-cleanup.wxs.
!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\dynamic-island.exe" uninstall-cleanup'
!macroend
