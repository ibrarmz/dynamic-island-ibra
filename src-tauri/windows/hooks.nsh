; Dynamic Island installer hooks (NSIS).
; Before uninstalling, undo what the app set up outside its own folder
; (Claude Code hooks in ~/.claude/settings.json, "start with Windows" entry).
; The MSI does the same through windows/msi-cleanup.wxs.
; Never during an in-app update (/UPDATE): the user keeps everything.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\dynamic-island.exe" uninstall-cleanup'
  ${EndIf}
!macroend
