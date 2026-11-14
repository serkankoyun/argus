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

## Graph
pane-graph = Live graph
graph-legend = ● task  ◆ msg  ✓ result  ▲ approval  ! finding · brighter edge = more traffic
pane-traffic = Traffic · 15 min
pane-now = Now
you = you
approvals-pending = { $n } approval(s) pending
press-a = press a to approve

## Room
pane-room = Who is doing what, and asking whom
now-label = now
pane-requests = Open requests
pane-conversations = Recent conversations
req-answered = ✓ ANSWERED
req-answered-in = answered in { $t }
req-approval = NEEDS APPROVAL
req-new = NEW
req-working = IN PROGRESS
req-waiting = waiting { $t }
req-running = running { $t }
req-none = No open requests.

## Lanes
pane-lanes = Agents and the messages between them
lanes-legend = Read · Edit · Bash · agents · web  │  ┊ message  │  z zoom ({ $w })
lanes-now = now
lanes-now-col = NOW / WAITING FOR
lanes-waiting-for = waiting for
lanes-asked-by = asked by
pane-matrix = Who asks whom for what
matrix-corner = from ↓ / to →

## Agent detail
agent-none = No agent selected. Pick one with ↑↓ in the Overview.
agent-model = model
agent-parent = parent
agent-project = project
agent-branch = branch
agent-context = Context
agent-ctx-used = used
agent-ctx-free = free
agent-compact = auto-compact
agent-compact-now = imminent
agent-compact-at = at { $p }%
agent-tokens = Tokens & cost (est.)
agent-input = input
agent-output = output
agent-cache-read = cache read
agent-cache-write = cache write
agent-cost = cost
agent-tools = tool calls
agent-errors = errors
agent-activity = Activity · tool calls/min · 30 min
agent-tool-calls = Tool calls
agent-files = Files touched
agent-no-files = No files edited yet.
agent-transcript = Transcript
col-target = TARGET
col-dur = DUR
col-status = STATUS
status-running = running

## Sequence
pane-sequence = Sequence · newest at the bottom
pane-tasks = Tasks
pane-message = Latest message
unassigned = unassigned
tasks-none = No tasks yet (Agent Teams task lists appear here).

## Fleet
col-region = REGION
col-collector = COLLECTOR
col-rtt = RTT
col-cpu = CPU
col-mem = MEM
col-agents = AGENTS
col-last = LAST EVENT
collector-ok = ok
collector-down = reconnecting
ago = { $t } ago
pane-alerts = Alerts
alert-waiting = { $what }
alert-stalled = stalled · { $what }
alert-context = context at { $p }% · compaction soon
alert-host-down = collector link down · buffering locally
alerts-none = All clear.
pane-hub = Data source
info-source = source
info-hosts = hosts
info-agents = agents
info-messages = messages
info-cost = cost (est.)
info-connect = Add machines: press : then type connect user@server
cost-by-agent = { $host } · cost by agent

## Channel
pane-channels = Channels
chan-approvals = approvals
chan-direct = direct messages
chan-hosts = hosts
chan-perm = { $who } NEEDS APPROVAL → you: { $what }   [a] approve  [r] reject
chan-input-hint = :msg <text> · a approve · > new run
pane-members = Members · now

## Task map
pane-taskmap = Dependencies and hand-offs
pane-bottlenecks = Bottlenecks
bottlenecks = Bottlenecks
bottlenecks-none = Nothing is blocked.
holds-n = holds up { $n } task(s)
critical-path = Critical path
tasks-progress = { $done } of { $total } tasks done
pane-utilization = Agent utilization · 15 min

## Pair
pair-none = No conversation partner yet.
pair-quiet = No message in flight.
pair-history = earlier messages
pair-channel = Channel between them
pair-others = Everyone else, now

## Commands
key-approve = approve
key-reject = reject
key-cmd = command
key-prompt = new run
input-cmd-hint = :connect user@host · :msg <text> · :run <dir> <prompt> · :view <name> · :lang tr · esc
input-prompt-hint = prompt for a new Claude Code run in the selected agent's project · enter to start
no-approvals = no pending approvals
unknown-cmd = unknown command: { $cmd }

## Teams
view-teams = Teams
team-label = team
key-team = next team
teams-agents = agents
teams-tasks = { $n } open tasks
teams-msgs = msgs 15m
teams-filtered = all views filtered to this team · t to change
teams-cross = Requests between teams
teams-from = FROM TEAM
teams-to = TO TEAM
teams-count = MSGS
teams-latest = LATEST
teams-open = open team
key-team-move = choose team
key-leave-team = all teams

## Hosts filter and history
host-label = host
key-host = host
key-show-done = show finished
key-hide-done = hide finished
fleet-hint = ↑↓ choose a host · enter shows only that host · s cycles hosts anywhere
view-history = History
history-title = Finished agents · { $n } · ${ $cost }
history-hint = finished agents are hidden from other views · f shows them there too
history-empty = No finished agents yet.
history-last = Last finished
col-team = TEAM
col-finished = FINISHED
col-outcome = OUTCOME
empty-hidden = { $n } sessions are hidden: they finished, or a host or team filter is on. Press f to show finished agents, esc to clear the filters, H for History.

## argus init
init-default-team = no-team
init-none = No team pattern found in { $n } agent names. Names shaped like role-team-work (for example qa-payments-refunds) can be grouped; otherwise write teams.json by hand (see INSTALL.md).
init-preview = Agents per team with these rules:
init-hint = Nothing saved. Run `argus init --write` to save these rules to { $path }, then edit the file if needed.
init-written = Saved to { $path }. Argus picks it up within a second; edit the file to rename teams or add labels.
init-missing-file = Warning: teams_file { $path } in your current teams.json does not exist, so its team codes and names are not used. Point it at an existing file.
