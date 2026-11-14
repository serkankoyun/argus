# Argus — English (default)
# Translators: copy this file to <lang>.ftl and translate the values.
# Keep the message ids and { $placeholders } unchanged.

## Views
view-overview = Overview
view-sequence = Sequence
view-agent = Agent
view-fleet = Fleet
view-graph = Graph
view-room = Room
view-lanes = Lanes
view-channel = Channel
view-taskmap = Task map
view-pair = Pair

placeholder = This view arrives in { $milestone }. Press 1 for the Overview.

## Header
header-summary = { $hosts } hosts · { $agents } agents · { $active } active
status-live = live
status-paused = paused

## Panes
pane-hosts = Hosts
pane-agents = Agents

## Agent table columns
col-agent = AGENT
col-host = HOST
col-state = STATE
col-tool = TOOL
col-tokens = TOKENS
col-context = CONTEXT
col-cost = COST
col-time = TIME

## Agent states
state-working = working
state-waiting = waiting
state-done = done
state-idle = idle
state-stalled = stalled

## Key hints
key-views = views
key-next = next view
key-select = select
key-pause = pause
key-quit = quit

## v0.1 additions
status-demo = demo
header-waiting = { $n } waiting
header-stalled = { $n } stalled
loading = Reading Claude Code sessions…
empty = No recent Claude Code sessions in { $dir }. Start one, or try `argus --demo`.
pane-flow = Flow · tasks & messages
pane-metrics = Metrics
pane-events = Events
flow-empty = No messages between agents yet.
m-tokens = tokens/s
m-cost = cost/h
m-tools = tools/min
m-msgs = msgs/min
m-errors = errors
m-ctx = ctx avg
key-detail = detail

## What an agent is doing now
now-thinking = thinking…
now-waiting = waiting for approval: { $what }
now-idle = waiting for input
now-done = finished
now-stalled = no progress ({ $what })

## Message kinds
kind-task = task
kind-msg = msg
kind-result = result
kind-perm = approval?
kind-finding = finding

## Task status
task-pending = PENDING
task-in-progress = IN PROGRESS
task-blocked = BLOCKED
task-done = DONE
