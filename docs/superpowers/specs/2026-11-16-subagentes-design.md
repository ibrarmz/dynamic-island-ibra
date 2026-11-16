# Subagentes en la isla — diseño

Aprobado por el usuario el 2026-11-16, tal cual el mockup
(https://claude.ai/artifact/EDrAzFcVBad2fkaYEuyJKB).

## Intención

Cuando Claude Code lanza subagentes (herramienta `Agent`/`Task`), la isla hoy
solo muestra DELEGANDO y después mezcla la actividad de los subagentes con la
de Claude. Queremos que la isla los entienda: cuántos hay, qué hace cada uno,
cuál pide permiso, y que no diga LISTO mientras queden agentes trabajando.

## Metas

- Contar los subagentes activos de la sesión y mostrarlos en la píldora y en el panel.
- Separar sus eventos: lo que hace un subagente no pisa la etiqueta, el detalle ni el diff de la sesión principal.
- Avisar qué agente pide permiso.
- Mostrar "LISTO · N AGENTES SIGUEN" cuando Claude terminó su turno con agentes vivos.

## No metas

- Mostrar el diff de cada subagente o su transcripción.
- Controlar los agentes desde la isla (pausar, cancelar).

## Anti-metas

- Romper la isla con versiones de Claude Code sin `SubagentStart` / `SubagentStop`: sin esos eventos debe comportarse como hoy.
- Despertar la UI de más: se sigue emitiendo `claude` solo cuando cambia algo.

## Datos disponibles (verificado en Claude Code 2.1.325)

- Todo hook disparado desde un subagente trae `agent_id` (ausente en el hilo principal).
- `SubagentStart`: `agent_id`, `agent_type`.
- `SubagentStop`: `agent_id`, `agent_type`, `agent_transcript_path`, `last_assistant_message`.
- La tarea (`description`) y el tipo (`subagent_type`) llegan antes, en el `PreToolUse` de la herramienta `Agent` del hilo principal.

## Enfoques

1. **Por `agent_id` en el core (elegido).** Rust guarda una lista de agentes por sesión; los eventos con `agent_id` actualizan solo a su agente. Exacto y barato.
2. Inferir agentes por pares `PreToolUse`/`PostToolUse` de la herramienta `Agent` sin hooks nuevos: no sabe qué hace cada agente ni cuándo terminan los que van en segundo plano.
3. Leer `agent_transcript_path`: más detalle, pero lectura de archivos y parseo frágil; fuera de alcance.

## Diseño

- **Hooks (`hooks.rs`):** agregar `SubagentStart` y `SubagentStop` a `EVENTS`. `repair()` reinstala también cuando a los hooks de la isla les falta algún evento de `EVENTS`, así los usuarios existentes reciben los nuevos sin reconectar.
- **Estado (`claude/mod.rs`):** `Session` gana `agents: Vec<Agent>` con `id`, `kind` (agent_type), `task`, `label`, `detail`, `status` (`working` | `waiting` | `done`), `started_at`, `ended_at`, `tools`, `message`.
  - `PreToolUse` de `Agent`/`Task` en el hilo principal: encola `(subagent_type, description)` como tarea pendiente.
  - `SubagentStart`: crea el agente y toma la primera tarea pendiente del mismo tipo (si no hay, la primera de cualquiera).
  - Evento con `agent_id` (`PreToolUse`, `PostToolUse`, `Notification`): actualiza solo ese agente (misma tabla de etiquetas LEYENDO/EDITANDO/… que la sesión); si el agente no existe (se perdió el `SubagentStart`), se crea.
  - `SubagentStop`: marca `done` y `ended_at`.
  - `UserPromptSubmit`: vacía la lista y las tareas pendientes.
  - Los agentes `done` se quedan en la lista para el panel; la cuenta "activos" es la de no `done`.
- **Píldora (`Pill.svelte`):** con agentes activos y Claude trabajando: DELEGANDO + contador con puntos. Claude `done` con agentes activos: "✓ LISTO · N AGENTES SIGUEN" (la actividad `claude-done` se mantiene mientras haya agentes activos). Agente pidiendo permiso: "Agente <tipo> pide permiso · <TOOL>" (actividad `claude-wait`).
- **Panel (`Claude.svelte`):** con agentes, la tarjeta central se reemplaza por la lista de agentes (máx. 3 visibles, activos primero): ícono de estado, tipo, tarea, acción actual, tiempo. Los `done` atenuados con ✓. Meta: "N agentes · M tools".

## Lo que queda al implementador

- Orden y truncado exacto de la lista cuando hay más de 3 agentes.
- Si el `peek` automático se dispara también cuando pide permiso un subagente (propuesta: sí, igual que con Claude).
