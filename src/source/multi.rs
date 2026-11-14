//! Combines several sources (local + remote hosts) into one snapshot.

use super::remote::RemoteSource;
use super::{Control, Notice, Source};
use crate::model::*;

pub struct MultiSource {
    sources: Vec<Box<dyn Source>>,
    metrics: Metrics,
    last: Option<(Millis, u64, f64)>,
}

impl MultiSource {
    pub fn new(sources: Vec<Box<dyn Source>>) -> Self {
        Self {
            sources,
            metrics: Metrics::default(),
            last: None,
        }
    }
}

impl Source for MultiSource {
    fn poll(&mut self) -> Snapshot {
        let now = now_millis();
        let mut out = Snapshot {
            taken_at: now,
            ..Default::default()
        };
        let mut sources = Vec::new();
        for s in &mut self.sources {
            let snap = s.poll();
            sources.push(snap.source.clone());
            out.hosts.extend(snap.hosts);
            out.agents.extend(snap.agents);
            out.messages.extend(snap.messages);
            out.tools.extend(snap.tools);
            out.tasks.extend(snap.tasks);
            out.team_labels.extend(snap.team_labels);
            out.warnings.extend(snap.warnings);
        }
        // With remote hosts, hide the local machine when it has no agents of its own.
        if self.sources.len() > 1 {
            let agents = &out.agents;
            out.hosts
                .retain(|h| h.region != "local" || agents.iter().any(|a| a.host == h.name));
        }
        out.messages.sort_by_key(|m| m.ts);
        out.tools.sort_by_key(|t| t.ts);
        out.agents
            .sort_by_key(|a| std::cmp::Reverse(a.last_activity));
        super::claude::update_metrics(&mut self.metrics, &mut self.last, &out, now);
        out.metrics = self.metrics.clone();
        out.source = sources.join(" + ");
        out
    }

    fn owns(&self, agent: &str) -> bool {
        self.sources.iter().any(|s| s.owns(agent))
    }

    fn control(&mut self, c: &Control) -> Notice {
        match c {
            Control::AddHost(target) => {
                self.sources
                    .push(Box::new(RemoteSource::new(target.clone())));
                Ok(format!("connecting to {target}"))
            }
            Control::Decide { agent, .. } | Control::Resume { agent, .. } => {
                match self.sources.iter_mut().find(|s| s.owns(agent)) {
                    Some(s) => s.control(c),
                    None => Err("unknown agent".into()),
                }
            }
            Control::Run { host, .. } => {
                let target = match host {
                    None => self.sources.first_mut(),
                    Some(h) => self
                        .sources
                        .iter_mut()
                        .find(|s| s.host().as_deref() == Some(h.as_str())),
                };
                match target {
                    Some(s) => s.control(c),
                    None => Err("unknown host".into()),
                }
            }
        }
    }
}
