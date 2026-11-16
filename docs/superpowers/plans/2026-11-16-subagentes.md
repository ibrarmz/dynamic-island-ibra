# Subagentes en la isla — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The island tracks Claude Code subagents (count, task, current action, permission waits) separately from the main session, as in the approved mockup.

**Architecture:** The hook client forwards `agent_id`/`agent_type`/`subagent_type`. The core keeps `agents: Vec<Agent>` per `Session`, updated by a new pure `apply()` (unit-tested, no Tauri) that `handle()` wraps. Two new hook events (`SubagentStart`, `SubagentStop`) are registered and `repair()` adds them for existing users. The Svelte pill and Claude panel render from the `agents` array of the `claude` event.

**Tech Stack:** Rust (Tauri 2, serde_json), Svelte 5.

**Spec:** `docs/superpowers/specs/2026-11-16-subagentes-design.md`

## Global Constraints

- User-facing text in Spanish; code comments and commit messages in English.
- `src-tauri/src/claude/hookclient.rs` stays std + serde_json only (shared with `island-hook`).
- Event-driven: `claude` is emitted from hook events only; no polling or timers in the core.
- Without `SubagentStart`/`SubagentStop` (older Claude Code) behaviour must equal today's.
- Build and test inside the `dev` distrobox: `distrobox enter dev -- bash -lc 'cd /var/home/ibra/Documentos/dev/dynamic-island-ibra/src-tauri && <cmd>'`. Frontend: `npm run build` on the host.
- Labels reuse the session's table: LEYENDO, EDITANDO, PROBANDO, EJECUTANDO, DELEGANDO, PLANEANDO, TRABAJANDO, ESPERA, LISTO.

## Review Focus

- Two subagents of the same type launched in parallel: each `SubagentStart` takes a different pending task (FIFO per type), never the same one twice.
- A subagent event whose `SubagentStart` was lost (island started mid-session): the agent is created on the fly with an empty task, nothing panics.
- A subagent asking permission while the main session is `done`: the pill shows the agent's permission message, not "LISTO".
- A new `UserPromptSubmit` clears the previous turn's agents and pending tasks.
- Subagent `Edit` events never touch `session.diff`, `session.label` or `session.detail`.

---

### Task 1: Forward subagent fields from the hook client

**Files:**
- Modify: `src-tauri/src/claude/hookclient.rs` (`slim`, plus a `#[cfg(test)] mod tests` at the end)

**Interfaces:**
- Produces: slimmed events carry top-level `agent_id`, `agent_type` (null when absent) and `tool_input.subagent_type`.

- [ ] **Step 1: Write the failing test** `slim_keeps_subagent_fields`

```rust
let ev = json!({"hook_event_name":"PreToolUse","session_id":"s","agent_id":"a1","agent_type":"Explore",
  "tool_name":"Agent","tool_input":{"description":"Buscar","subagent_type":"Explore","prompt":"x"}});
let m = slim(&ev);
assert_eq!(m["agent_id"], "a1");
assert_eq!(m["agent_type"], "Explore");
assert_eq!(m["tool_input"]["subagent_type"], "Explore");
assert!(m["tool_input"].get("prompt").is_none()); // still slimmed
```

- [ ] **Step 2: Run** `cargo test -p dynamic-island hookclient` — Expected: FAIL (`agent_id` is null).
- [ ] **Step 3: Implement** — add `"subagent_type"` to the kept `tool_input` keys and `"agent_id"`, `"agent_type"` to the output object.
- [ ] **Step 4: Run** `cargo test -p dynamic-island hookclient` and `cargo build -p island-hook` — Expected: PASS, builds.
- [ ] **Step 5: Commit** `feat: forward subagent fields from the hook client`

### Task 2: Track subagents in the core

**Files:**
- Modify: `src-tauri/src/claude/mod.rs`

**Interfaces:**
- Consumes: Task 1 fields.
- Produces (serialized in the `claude` event, read by Tasks 4–5):
  - `Session.agents: Vec<Agent>`; `Agent { id: String, kind: String, task: String, label: &'static str, detail: String, status: &'static str /* "working"|"waiting"|"done" */, started_at: u64, ended_at: Option<u64>, tools: u32, message: Option<String> }`.
  - `#[serde(skip)] Session.pending: Vec<(String, String)>` — `(subagent_type, description)`.
  - `pub(crate) struct Applied { pub changed: bool, pub peek: bool }` and `pub(crate) fn apply(map: &mut HashMap<String, Session>, ev: &Value, now: u64) -> Applied`; `handle()` = lock, `apply`, emit `claude` (and `peek` when `peek`).

- [ ] **Step 1: Write failing tests** in `#[cfg(test)] mod tests` (build events with `json!`, `now` fixed, `ts` increasing):
  - `main_flow_unchanged`: UserPromptSubmit → PreToolUse Read → Stop gives `status=="done"`, `label=="LISTO"`, `agents` empty.
  - `agent_tool_queues_task_and_start_takes_it`: PreToolUse `Agent` `{description:"Buscar RAM", subagent_type:"Explore"}` then `SubagentStart {agent_id:"a1", agent_type:"Explore"}` → one agent, `task=="Buscar RAM"`, `status=="working"`, `pending` empty.
  - `parallel_same_type_fifo`: two `Agent` calls (tasks "A","B", both Explore), two starts a1,a2 → a1.task=="A", a2.task=="B".
  - `agent_events_do_not_touch_session`: after a1 starts, PreToolUse Edit with `agent_id:"a1"` → `agents[0].label=="EDITANDO"`, `agents[0].tools==1`; `session.diff` is None and `session.label` unchanged.
  - `unknown_agent_created`: PreToolUse Read with `agent_id:"zz"`, `agent_type:"Plan"` and no start → agent zz exists, `kind=="Plan"`, `task==""`.
  - `agent_permission_peeks`: Notification `{agent_id:"a1", notification_type:"permission_prompt", message:"Claude needs your permission to use Bash"}` → agent `status=="waiting"`, `label=="ESPERA"`, `Applied.peek`; then PostToolUse with `agent_id:"a1"` → back to `"working"`.
  - `subagent_stop_marks_done`: SubagentStop a1 → `status=="done"`, `label=="LISTO"`, `ended_at==Some(now)`.
  - `new_prompt_clears_agents`: UserPromptSubmit after agents exist → `agents` and `pending` empty.
- [ ] **Step 2: Run** `cargo test -p dynamic-island claude::tests` — Expected: compile errors / FAIL.
- [ ] **Step 3: Implement.** Extract the current body of `handle()` into `apply()` unchanged, then add: in `apply`, if `ev["agent_id"]` is a non-empty string and the event is PreToolUse/PostToolUse/Notification, route it to that agent (find or create with `kind` = `ev["agent_type"]`) and return without touching the session's fields except `updated_at`/`last_ts`. Move the tool→(label, detail) mapping out of `Session::tool_use` into `fn describe_tool(tool: &str, input: &Value) -> (Option<usize>, &'static str, String)` used by both session and agents (agents ignore phase and diff). `PreToolUse` of `Agent`/`Task` on the main thread also pushes `(subagent_type, description)` to `pending`. `SubagentStart`: take the first pending entry whose type equals `agent_type`, else the first entry, else empty task. `start_task` clears `agents` and `pending`.
- [ ] **Step 4: Run** `cargo test -p dynamic-island claude::tests` then `cargo build` — Expected: all PASS, no warnings.
- [ ] **Step 5: Commit** `feat: track Claude Code subagents per session`

### Task 3: Register the new hook events and repair existing installs

**Files:**
- Modify: `src-tauri/src/claude/hooks.rs`

**Interfaces:**
- Produces: `EVENTS` gains `("SubagentStart", false)`, `("SubagentStop", false)`; `fn missing_events(settings: &Value) -> bool` (true when any of our hooks exists but some `EVENTS` entry has none of ours).

- [ ] **Step 1: Write failing tests**: `missing_events_detects_new_events` (settings with our hook only under `PreToolUse` → true), `missing_events_false_when_complete` (our hook under every `EVENTS` name → false), `missing_events_false_when_not_connected` (no hooks → false).
- [ ] **Step 2: Run** `cargo test -p dynamic-island hooks` — Expected: FAIL.
- [ ] **Step 3: Implement** `missing_events` and make `repair()` call `install()` when `!hooks.is_empty() && (stale || missing_events(&settings))`. Update the `repair` doc comment.
- [ ] **Step 4: Run** `cargo test -p dynamic-island hooks` — Expected: PASS.
- [ ] **Step 5: Commit** `feat: hook SubagentStart/SubagentStop and add them on repair`

### Task 4: Pill states for subagents

**Files:**
- Modify: `src/lib/store.svelte.js` (helpers), `src/App.svelte` (`acts`, `pillW`), `src/lib/Pill.svelte`

**Interfaces:**
- Consumes: `claude.session.agents` (Task 2).
- Produces: `export const activeAgents = () => (claude.session?.agents ?? []).filter((a) => a.status !== 'done')` and `export const waitingAgent = () => activeAgents().find((a) => a.status === 'waiting') ?? null` in the store.

- [ ] **Step 1: Implement** in `App.svelte`: `claude-wait` when `cStatus === 'waiting' || waitingAgent()`; `claude-done` when `doneFresh || (cStatus === 'done' && activeAgents().length > 0)`; pill width 380 when agents are active.
- [ ] **Step 2: Implement** in `Pill.svelte`, full layout only:
  - `claude-work` with active agents: spinner + `DELEGANDO` on the left; right: badge with three blinking dots + active count, then elapsed.
  - `claude-done` with active agents: check + `LISTO` on the left; right: small spinner + `{n} AGENTE(S) SIGUEN` (`1 AGENTE SIGUE` singular).
  - `claude-wait` from an agent: pulse + `Agente <span>{kind}</span> pide permiso`, right: tool from the agent's message (same regex as today) or `PERMISO`.
  - Compact (two activities) layouts keep today's look.
- [ ] **Step 3: Verify** `npm run build` (no errors) and run the dev app (`npm run tauri dev` inside distrobox, or the debug binary + `npx vite`), feed events with `echo '<json>' | src-tauri/target/debug/island-hook` in this order: UserPromptSubmit, PreToolUse Agent ×2 (Explore), SubagentStart a1/a2, PreToolUse Read (agent a1) — screenshot shows `DELEGANDO` + `2`; Stop — shows `LISTO · 2 AGENTES SIGUEN`; Notification permission (agent a1) — shows `Agente Explore pide permiso · BASH`.
- [ ] **Step 4: Commit** `feat: pill shows subagents`

### Task 5: Agent list in the Claude panel

**Files:**
- Modify: `src/lib/Claude.svelte`

**Interfaces:**
- Consumes: `activeAgents()` (Task 4), `claude.session.agents`.

- [ ] **Step 1: Implement**: when `s.agents.length > 0` and the session isn't `waiting`, the centre card becomes the agent list (active first, then done; max 3 rows): status icon (spinner / pulse / green ✓), `kind` upper-case, `task` (fallback `Subagente`), `label detail` underneath (`PIDE PERMISO · <TOOL>` when waiting), elapsed (`fmt.short`, `(ended_at ?? clock.now) - started_at`); done rows at 0.6 opacity; waiting row orange-tinted. Meta line: `{n} agentes · {tools} tools`. Phases bar is hidden while the list shows.
- [ ] **Step 2: Verify** `npm run build`; replay Task 4's events with `ISLAND_PEEK=claude` and screenshot the panel after the starts and after Stop: rows match the mockup boards 4–5.
- [ ] **Step 3: Commit** `feat: Claude panel lists subagents`

### Task 6: Docs and end-to-end check

**Files:**
- Modify: `CLAUDE.md` (Claude Code integration bullets: new events, `agent_id` routing, `apply()` tests, `cargo test` command)

- [ ] **Step 1: Update CLAUDE.md**; mention `cargo test -p dynamic-island` under Commands (there are unit tests now).
- [ ] **Step 2: Verify** with real Claude Code: start the dev app, confirm `repair()` added `SubagentStart`/`SubagentStop` to `~/.claude/settings.json` (our command under both keys), then launch a real subagent from a Claude session and screenshot the pill and panel while it runs and after it finishes.
- [ ] **Step 3: Commit** `docs: subagent tracking in CLAUDE.md`
