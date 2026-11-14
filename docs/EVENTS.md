# Argus event format (v1)

Claude Code writes a transcript line only when a step finishes. During a long `Bash`
command, or while a session waits for you, the transcript is silent and Argus can only
guess. Lifecycle events remove the guessing: one JSON object per line, appended to a
`.jsonl` file, that says what a session is doing **now**.

Any tool can write them: an orchestrator, a wrapper script, a CI job. Argus reads:

| Directory | Written by |
|---|---|
| `~/.claude/argus/events/` | `argus hook --event`, installed by `argus hook --install` |
| `~/.agent-orchestra/events/` | agent-orchestra (same format) |
| every directory in `ARGUS_EVENTS` | your own tools (a path list, `:`-separated; `;` on Windows) |

Files modified in the last 24 hours are read, incrementally. File names are free; Argus
writes `events-YYYY-MM-DD.jsonl` and deletes its own files after 3 days.

## One event

```json
{"v":1,"ts":"2026-11-14T10:43:25.767Z","event":"PreToolUse","sid":"1b4bba2e-…","tool":"Bash"}
```

| Field | Required | Meaning |
|---|---|---|
| `v` | no | Format version, `1` |
| `ts` | yes | RFC 3339 time or epoch milliseconds |
| `event` | yes | One of the events below (Claude Code hook names) |
| `sid` | yes | Session id: the transcript file name under `~/.claude/projects/` |
| `agentId` | no | Subagent id (the `agent-<id>.jsonl` file of that session) |
| `tool` | no | Tool name for tool events |
| `status` | no | For `Notification`: `idle_prompt`, `permission_prompt`, … |
| `mode` | no | Permission mode; `auto`, `bypassPermissions` and `dontAsk` mean the session never waits for approvals |

Unknown fields are ignored. `session_id`, `agent_id`, `tool_name`, `hook_event_name` and
`notification_type` are accepted as aliases, so raw Claude Code hook payloads with a `ts`
work too.

## How events change what Argus shows

| Event | Effect |
|---|---|
| `PreToolUse` | working, running that tool (until `PostToolUse`); never shown as "waiting for approval" |
| `PostToolUse`, `UserPromptSubmit`, `SessionStart` | working |
| `PermissionRequest`, `Notification` with `permission…` | waiting for approval |
| `Notification` with `idle…`, `Stop` | idle, waiting for a prompt |
| `SessionEnd` | finished (moves to History) |
| `SubagentStop` with `agentId` | that subagent finished |

Events that carry `agentId` describe that subagent (`PreToolUse` with `agentId` means the
subagent is running a tool), so subagents get exact states too.

When a transcript line is newer than the last event, the transcript wins.
