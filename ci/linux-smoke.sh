#!/usr/bin/env bash
# Smoke test of the Linux build on a virtual screen: starts the island, feeds it
# Claude hook events through island-hook (Unix socket), screenshots it and
# reports memory (PSS of the whole process tree). Output goes to /out.
set -u
BIN=${BIN:-/target/release}
export XDG_RUNTIME_DIR=/tmp/rt && mkdir -p -m 700 "$XDG_RUNTIME_DIR"
export DISPLAY=:99 NO_AT_BRIDGE=1
Xvfb :99 -screen 0 1920x1080x24 >/dev/null 2>&1 &
sleep 1

dbus-run-session -- bash -c '
  ISLAND_PEEK=claude "$0/dynamic-island" >/tmp/island.log 2>&1 &
  sleep 8
  hook() { echo "$1" | "$0/island-hook"; echo "hook $2 exit=$?"; }
  hook "{\"session_id\":\"s1\",\"cwd\":\"/home/u/dynamic-island\",\"hook_event_name\":\"UserPromptSubmit\",\"prompt\":\"Haz que la isla sea de vidrio\"}" prompt
  hook "{\"session_id\":\"s1\",\"cwd\":\"/home/u/dynamic-island\",\"hook_event_name\":\"PreToolUse\",\"tool_name\":\"Edit\",\"tool_input\":{\"file_path\":\"/home/u/dynamic-island/src/glass.ts\",\"old_string\":\"const a = 1;\",\"new_string\":\"const a = 2;\\nconst b = 3;\"}}" edit
  sleep 2
  import -window root -crop 760x330+580+0 /out/linux-claude.png && echo "screenshot ok"
  echo "--- processes / memory (PSS) ---"
  total=0
  for p in $(pgrep -f "dynamic-island|WebKit"); do
    kb=$(awk "/^Pss:/{print \$2}" /proc/$p/smaps_rollup 2>/dev/null || echo 0)
    printf "%-28s %6s kB\n" "$(cat /proc/$p/comm)" "$kb"; total=$((total + ${kb:-0}))
  done
  echo "TOTAL PSS: $((total / 1024)) MB"
  echo "--- second instance must exit immediately ---"
  timeout 5 "$0/dynamic-island"; echo "second instance exit=$?"
  echo "--- island log ---"; tail -n 20 /tmp/island.log
' "$BIN"
