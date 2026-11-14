//! Combines several sources (local + remote hosts) into one snapshot.

use super::Source;
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
}
