//! Demo source: a scripted, looping simulation of a small multi-host team.
//! Used by `argus --demo`, for screenshots, and whenever no real data exists.

use super::pricing;
use super::Source;
use crate::model::*;

const PERIOD: i64 = 48_000;

struct Script {
    at: i64,
    from: &'static str,
    to: &'static str,
    kind: MsgKind,
    label: [&'static str; 2],
    text: [&'static str; 2],
}

const SCRIPT: &[Script] = &[
    Script { at: 0, from: "lead", to: "api-dev", kind: MsgKind::Task,
        label: ["add rate limit to /login (#18)", "login'e hız sınırı ekle (#18)"],
        text: ["Add a limit of 10 requests per minute per IP to the login endpoint. Redis is already set up. Send the tests to test-runner when done.",
               "Login endpoint'ine IP başına dakikada 10 istek sınırı ekle. Redis zaten var. Bitince testleri test-runner'a gönder."] },
    Script { at: 5_000, from: "api-dev", to: "db-dev", kind: MsgKind::Msg,
        label: ["is users.email unique?", "users.email alanı unique mi?"],
        text: ["Is users.email unique? I need to return 409 on duplicate sign-ups.",
               "users.email alanı unique mi? Aynı e-postada 409 dönebilmem lazım."] },
    Script { at: 9_000, from: "db-dev", to: "api-dev", kind: MsgKind::Msg,
        label: ["yes, via migration 0042", "evet, 0042 migration'ı ile"],
        text: ["Yes, migration 0042 makes it unique, but it is not applied in prod yet — waiting for approval.",
               "Evet, 0042 migration'ı ile unique oluyor ama prod'da henüz uygulanmadı, onay bekliyorum."] },
    Script { at: 13_000, from: "db-dev", to: YOU, kind: MsgKind::Perm,
        label: ["Bash: psql -f 0042_unique_email.sql", "Bash: psql -f 0042_unique_email.sql"],
        text: ["May I run 0042_unique_email.sql against the production database?",
               "0042_unique_email.sql dosyasını prod veritabanında çalıştırayım mı?"] },
    Script { at: 18_000, from: "lead", to: "test-runner", kind: MsgKind::Task,
        label: ["run the auth suite (#16)", "auth testlerini koş (#16)"],
        text: ["Run the auth test suite and report back.", "Auth test paketini koş, sonucu bana bildir."] },
    Script { at: 24_000, from: "test-runner", to: "lead", kind: MsgKind::Result,
        label: ["42 passed · 1 failed", "42 geçti · 1 kırık"],
        text: ["Auth suite finished: 42 passed, 1 failed (login returns 500 on a duplicate email).",
               "Auth paketi bitti: 42 geçti, 1 kırık (login aynı e-postada 500 dönüyor)."] },
    Script { at: 28_000, from: "sec-audit", to: "reviewer", kind: MsgKind::Finding,
        label: ["HIGH: SQL injection reports.py:88", "YÜKSEK: reports.py:88 SQL injection"],
        text: ["reports.py:88 builds SQL with an f-string. PR #231 should not be merged as is.",
               "reports.py:88'de f-string ile SQL kuruluyor. PR #231 bu haliyle merge olmamalı."] },
    Script { at: 32_000, from: "lead", to: "reviewer", kind: MsgKind::Task,
        label: ["review PR #231", "PR #231'i incele"],
        text: ["Review PR #231 for security and performance.", "PR #231'i güvenlik ve performans açısından incele."] },
    Script { at: 37_000, from: "perf-probe", to: "reviewer", kind: MsgKind::Result,
        label: ["p95 latency 182ms", "p95 gecikme 182ms"],
        text: ["Load test done: p95 latency 182ms, no regressions.", "Yük testi bitti: p95 gecikme 182ms, gerileme yok."] },
    Script { at: 41_000, from: "reviewer", to: "sec-audit", kind: MsgKind::Msg,
        label: ["scan api/ as well", "api/ klasörünü de tara"],
        text: ["Thanks — please scan the api/ folder as well.", "Teşekkürler, api/ klasörünü de tarar mısın?"] },
    Script { at: 44_000, from: "test-runner", to: "api-dev", kind: MsgKind::Msg,
        label: ["why does login return 500?", "login neden 500 dönüyor?"],
        text: ["The duplicate-email test gets a 500 from /login. Can you take a look?",
               "Aynı e-posta testi /login'den 500 alıyor. Bakabilir misin?"] },
];

struct DemoAgent {
    id: &'static str,
    host: &'static str,
    parent: Option<&'static str>,
    state: AgentState,
    model: &'static str,
    tokens_per_sec: f64,
    base_tokens: u64,
    started_ago_s: i64,
    tools: &'static [(&'static str, &'static str)],
    tool_every_ms: i64,
}

const AGENTS: &[DemoAgent] = &[
    DemoAgent {
        id: "lead",
        host: "prod-1",
        parent: None,
        state: AgentState::Working,
        model: "claude-opus-5-5",
        tokens_per_sec: 18.0,
        base_tokens: 84_000,
        started_ago_s: 2460,
        tool_every_ms: 7_000,
        tools: &[
            ("Task", "add rate limit to /login"),
            ("Read", "docs/ROADMAP.md"),
            ("TaskCreate", "#19 add audit log"),
        ],
    },
    DemoAgent {
        id: "api-dev",
        host: "prod-1",
        parent: Some("lead"),
        state: AgentState::Working,
        model: "claude-sonnet-5-5",
        tokens_per_sec: 26.0,
        base_tokens: 38_000,
        started_ago_s: 1680,
        tool_every_ms: 4_000,
        tools: &[
            ("Read", "src/middleware/rateLimit.ts"),
            ("Edit", "src/api/routes/users.ts"),
            ("Grep", "\"redis\" in src"),
            ("Bash", "npm test -- auth/login"),
            ("Write", "src/middleware/rateLimit.ts"),
        ],
    },
    DemoAgent {
        id: "db-dev",
        host: "prod-1",
        parent: Some("lead"),
        state: AgentState::Waiting,
        model: "claude-sonnet-5-5",
        tokens_per_sec: 2.0,
        base_tokens: 22_000,
        started_ago_s: 1680,
        tool_every_ms: 30_000,
        tools: &[("Bash", "psql -f 0042_unique_email.sql")],
    },
    DemoAgent {
        id: "test-runner",
        host: "prod-1",
        parent: Some("lead"),
        state: AgentState::Working,
        model: "claude-haiku-5-5",
        tokens_per_sec: 12.0,
        base_tokens: 12_000,
        started_ago_s: 540,
        tool_every_ms: 6_000,
        tools: &[
            ("Bash", "npm test -- auth"),
            ("Read", "test-results/auth.log"),
        ],
    },
    DemoAgent {
        id: "reviewer",
        host: "gpu-2",
        parent: None,
        state: AgentState::Working,
        model: "claude-opus-5-5",
        tokens_per_sec: 15.0,
        base_tokens: 51_000,
        started_ago_s: 1980,
        tool_every_ms: 5_000,
        tools: &[
            ("Read", "PR #231 diff"),
            ("Read", "src/reports.py"),
            ("Grep", "\"execute(\" in src"),
        ],
    },
    DemoAgent {
        id: "sec-audit",
        host: "gpu-2",
        parent: Some("reviewer"),
        state: AgentState::Working,
        model: "claude-sonnet-5-5",
        tokens_per_sec: 9.0,
        base_tokens: 19_000,
        started_ago_s: 840,
        tool_every_ms: 4_500,
        tools: &[
            ("Grep", "\"execute(f\" in **/*.py"),
            ("Read", "src/reports.py"),
        ],
    },
    DemoAgent {
        id: "perf-probe",
        host: "gpu-2",
        parent: Some("reviewer"),
        state: AgentState::Done,
        model: "claude-haiku-5-5",
        tokens_per_sec: 0.0,
        base_tokens: 9_800,
        started_ago_s: 360,
        tool_every_ms: 0,
        tools: &[],
    },
    DemoAgent {
        id: "docs-writer",
        host: "gpu-2",
        parent: None,
        state: AgentState::Idle,
        model: "claude-sonnet-5-5",
        tokens_per_sec: 0.0,
        base_tokens: 30_000,
        started_ago_s: 3120,
        tool_every_ms: 0,
        tools: &[],
    },
    DemoAgent {
        id: "migrator",
        host: "dev-3",
        parent: None,
        state: AgentState::Stalled,
        model: "claude-sonnet-5-5",
        tokens_per_sec: 0.0,
        base_tokens: 61_700,
        started_ago_s: 4320,
        tool_every_ms: 0,
        tools: &[("WebFetch", "https://legacy.internal/ids/export")],
    },
];

pub struct DemoSource {
    start: Millis,
    lang: usize,
    metrics: Metrics,
}

impl DemoSource {
    pub fn new(lang: &str) -> Self {
        Self {
            start: now_millis(),
            lang: usize::from(lang == "tr"),
            metrics: Metrics::default(),
        }
    }
}

fn wave(t: f64, period: f64, phase: f64) -> f64 {
    (t / period * std::f64::consts::TAU + phase).sin()
}

impl Source for DemoSource {
    fn poll(&mut self) -> Snapshot {
        let now = now_millis();
        let el = now - self.start;
        let t = el as f64 / 1000.0;
        let l = self.lang;
        let mut snap = Snapshot {
            taken_at: now,
            source: "demo".into(),
            ..Default::default()
        };

        snap.hosts = vec![
            Host {
                name: "prod-1".into(),
                region: "eu-central".into(),
                connected: true,
                cpu_pct: (58.0 + 6.0 * wave(t, 17.0, 0.0)) as u8,
                mem_pct: 47,
                rtt_ms: Some(12),
            },
            Host {
                name: "gpu-2".into(),
                region: "eu-west".into(),
                connected: true,
                cpu_pct: (81.0 + 5.0 * wave(t, 13.0, 1.0)) as u8,
                mem_pct: 64,
                rtt_ms: Some(31),
            },
            Host {
                name: "dev-3".into(),
                region: "tr-ist".into(),
                connected: false,
                cpu_pct: 17,
                mem_pct: 22,
                rtt_ms: Some(248),
            },
        ];

        // Messages: replay the script for the current and the previous loops.
        let cycle = el / PERIOD;
        for k in (cycle - 3).max(0)..=cycle {
            for s in SCRIPT {
                let ts = self.start + k * PERIOD + s.at;
                if ts <= now {
                    snap.messages.push(Message {
                        ts,
                        from: s.from.into(),
                        to: s.to.into(),
                        kind: s.kind,
                        label: s.label[l].into(),
                        text: s.text[l].into(),
                        answered_at: (s.kind != MsgKind::Perm).then_some(ts + 3_000),
                    });
                }
            }
        }

        for a in AGENTS {
            let started = now - a.started_ago_s * 1000;
            let tokens = a.base_tokens + (a.tokens_per_sec * t) as u64;
            let usage = Usage {
                input: tokens / 10,
                output: tokens / 5,
                cache_read: tokens * 6,
                cache_write: tokens - tokens / 10 - tokens / 5,
            };
            let (tool, target) = if a.tools.is_empty() {
                ("—", "")
            } else if a.tool_every_ms > 0 {
                a.tools[((el / a.tool_every_ms) as usize) % a.tools.len()]
            } else {
                a.tools[0]
            };
            if a.tool_every_ms > 0 && !a.tools.is_empty() {
                let n = el / a.tool_every_ms;
                for k in (n - 12).max(0)..=n {
                    let (tl, tg) = a.tools[(k as usize) % a.tools.len()];
                    let ts = self.start + k * a.tool_every_ms + (a.id.len() as i64 * 137) % 900;
                    if ts <= now {
                        let failed = tl == "Bash" && a.id == "api-dev" && k % 3 == 0;
                        snap.tools.push(ToolCall {
                            ts,
                            agent: a.id.into(),
                            tool: tl.into(),
                            target: tg.into(),
                            duration_ms: Some(if tl == "Bash" {
                                8_400
                            } else {
                                40 + (k % 7) * 30
                            }),
                            ok: Some(!failed),
                        });
                    }
                }
            }
            let ctx_base: u64 = match a.id {
                "lead" => 148_000,
                "api-dev" => 122_000,
                "db-dev" => 76_000,
                "test-runner" => 44_000,
                "reviewer" => 132_000,
                "sec-audit" => 70_000,
                "perf-probe" => 36_000,
                "docs-writer" => 94_000,
                _ => 178_000,
            };
            let transcript = demo_transcript(a.id, l, now);
            let files = if a.id == "api-dev" {
                vec![
                    ("src/api/routes/users.ts".into(), 66, 12),
                    ("src/middleware/rateLimit.ts".into(), 41, 0),
                    ("src/api/routes/auth.ts".into(), 19, 17),
                    ("tests/auth/login.test.ts".into(), 12, 5),
                ]
            } else {
                vec![]
            };
            snap.agents.push(Agent {
                id: a.id.into(),
                name: a.id.into(),
                host: a.host.into(),
                project: "shop".into(),
                model: a.model.into(),
                parent: a.parent.map(Into::into),
                team: Some("auth-refactor".into()),
                state: a.state,
                tool: tool.into(),
                now: target.into(),
                usage,
                context_tokens: ctx_base + (a.tokens_per_sec * t * 4.0) as u64 % 20_000,
                context_window: 200_000,
                cost_usd: pricing::cost(a.model, &usage),
                started,
                last_activity: now
                    - if a.state == AgentState::Working {
                        800
                    } else {
                        120_000
                    },
                tool_calls: (tokens / 900) as u32,
                errors: u32::from(a.id == "api-dev"),
                git_branch: "feat/auth-refactor".into(),
                files,
                transcript,
            });
        }
        snap.tools.sort_by_key(|t| t.ts);

        snap.tasks = vec![
            task(
                "13",
                "split auth module",
                Some("lead"),
                TaskStatus::Done,
                &[],
            ),
            task(
                "17",
                "users schema v2",
                Some("db-dev"),
                TaskStatus::Done,
                &[],
            ),
            task("12", "redis setup", Some("api-dev"), TaskStatus::Done, &[]),
            task(
                "16",
                "auth test suite",
                Some("test-runner"),
                TaskStatus::InProgress,
                &["13"],
            ),
            task(
                "14",
                "apply migration 0042",
                Some("db-dev"),
                TaskStatus::Blocked,
                &["17"],
            ),
            task(
                "18",
                "rate limit /login",
                Some("api-dev"),
                TaskStatus::InProgress,
                &["12", "13"],
            ),
            task(
                "15",
                "migrate legacy ids",
                Some("migrator"),
                TaskStatus::Blocked,
                &["14"],
            ),
            task(
                "21",
                "review PR #231",
                Some("reviewer"),
                TaskStatus::InProgress,
                &["16", "18"],
            ),
            task(
                "19",
                "add audit log",
                None,
                TaskStatus::Pending,
                &["18", "14"],
            ),
            task(
                "22",
                "release v2.3",
                None,
                TaskStatus::Pending,
                &["21", "19", "15"],
            ),
        ];

        let m = &mut self.metrics;
        m.tokens_per_sec
            .push(400.0 + 80.0 * wave(t, 11.0, 0.0) + 30.0 * wave(t, 3.0, 2.0));
        m.cost_per_hour.push(6.9 + 0.8 * wave(t, 23.0, 1.0));
        m.tools_per_min.push(37.0 + 6.0 * wave(t, 7.0, 0.5));
        m.msgs_per_min.push(14.0 + 4.0 * wave(t, 9.0, 1.5));
        m.errors.push(if (el / 1000) % 17 == 0 {
            3.0
        } else {
            (1.0 + wave(t, 5.0, 0.0)).max(0.0).round()
        });
        m.ctx_avg.push(53.0 + 4.0 * wave(t, 31.0, 0.0));
        snap.metrics = m.clone();
        snap
    }
}

fn task(
    id: &str,
    title: &str,
    owner: Option<&str>,
    status: TaskStatus,
    blocked_by: &[&str],
) -> Task {
    Task {
        id: id.into(),
        title: title.into(),
        owner: owner.map(Into::into),
        status,
        blocked_by: blocked_by.iter().map(|s| s.to_string()).collect(),
    }
}

fn demo_transcript(id: &str, l: usize, now: Millis) -> Vec<(Millis, String, String)> {
    let lines: &[(&str, [&str; 2])] = match id {
        "api-dev" => &[
            ("user", ["Add rate limiting to /login (#18)", "/login'e hız sınırı ekle (#18)"]),
            ("assistant", ["The test expects 409 on a duplicate email; the route returns 500. Let me confirm the unique constraint first.",
                           "Test aynı e-postada 409 bekliyor; route şu an 500 dönüyor. Önce unique kısıtını teyit edeyim."]),
            ("tool", ["SendMessage → db-dev", "SendMessage → db-dev"]),
            ("user", ["db-dev: yes, via migration 0042 (awaiting approval)", "db-dev: evet, 0042 ile (onay bekliyor)"]),
            ("assistant", ["I'll add a sliding-window limiter using the existing Redis client, then fix the 409.",
                           "Mevcut Redis client'ı kullanıp sliding-window limiter ekliyorum, ardından 409 düzeltmesi."]),
            ("tool", ["Edit src/api/routes/users.ts", "Edit src/api/routes/users.ts"]),
        ],
        _ => &[],
    };
    lines
        .iter()
        .enumerate()
        .map(|(i, (role, text))| {
            (
                now - (lines.len() - i) as i64 * 4_000,
                role.to_string(),
                text[l].to_string(),
            )
        })
        .collect()
}
