//! Node placement shared by the graph-style views (Graph, Room).

use ratatui::layout::Rect;

use crate::model::{Snapshot, YOU};

#[derive(Debug, Clone)]
pub struct Node {
    /// Agent id, or [`YOU`] for the human operator.
    pub id: String,
    /// Center, in absolute terminal cells.
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub host: String,
    pub area: Rect,
}

/// Places agents in one column band per host; inside a band, depth in the
/// agent tree decides the row and siblings spread horizontally.
pub fn place(
    s: &Snapshot,
    area: Rect,
    node_w: u16,
    node_h: u16,
    with_you: bool,
    by_team: bool,
) -> (Vec<Node>, Vec<Group>) {
    // Hosts in order of first appearance (agents are sorted by activity).
    let mut hosts: Vec<(String, Vec<(usize, usize)>)> = Vec::new();
    for (idx, depth) in s.tree_order() {
        let a = &s.agents[idx];
        let host = if by_team {
            a.team_key()
        } else {
            a.host.clone()
        };
        let host = &host;
        match hosts.iter_mut().find(|(h, _)| h == host) {
            Some((_, v)) => v.push((idx, depth)),
            None => hosts.push((host.clone(), vec![(idx, depth)])),
        }
    }
    let mut nodes = Vec::new();
    let mut groups = Vec::new();
    if hosts.is_empty() {
        return (nodes, groups);
    }
    let reserve = if with_you { node_h + 1 } else { 0 };
    let bands = hosts.len() as u16;
    let band_w = area.width / bands.max(1);
    for (bi, (host, members)) in hosts.into_iter().enumerate() {
        let bx = area.x + band_w * bi as u16;
        let band = Rect {
            x: bx,
            y: area.y,
            width: band_w,
            height: area.height.saturating_sub(reserve),
        };
        groups.push(Group { host, area: band });
        // Split each depth level into rows of as many nodes as fit.
        let per_row = ((band.width.saturating_sub(2)) / (node_w + 1)).max(1) as usize;
        let max_depth = members.iter().map(|m| m.1).max().unwrap_or(0);
        let mut rows: Vec<Vec<usize>> = Vec::new();
        for depth in 0..=max_depth {
            let level: Vec<usize> = members
                .iter()
                .filter(|m| m.1 == depth)
                .map(|m| m.0)
                .collect();
            for chunk in level.chunks(per_row) {
                rows.push(chunk.to_vec());
            }
        }
        let n_rows = rows.len().max(1) as u16;
        let row_h = (band.height.saturating_sub(2) / n_rows).max(node_h);
        for (ri, row) in rows.into_iter().enumerate() {
            let y = band.y + 1 + row_h * ri as u16 + row_h / 2;
            let slot = band.width / row.len().max(1) as u16;
            for (k, idx) in row.into_iter().enumerate() {
                let x = band.x + slot * k as u16 + slot / 2;
                let x = x.clamp(
                    band.x + node_w / 2 + 1,
                    (band.x + band.width)
                        .saturating_sub(node_w / 2 + 1)
                        .max(band.x + node_w / 2 + 1),
                );
                nodes.push(Node {
                    id: s.agents[idx].id.clone(),
                    x,
                    y: y.max(band.y + node_h / 2 + 1),
                });
            }
        }
    }
    if with_you {
        nodes.push(Node {
            id: YOU.into(),
            x: area.x + area.width.saturating_sub(node_w / 2 + 2),
            y: area.y + area.height.saturating_sub(node_h / 2 + 1),
        });
    }
    (nodes, groups)
}

/// Team hierarchy: coordinators centered on top, one cluster per team below
/// (busiest first), and inside a cluster each depth of the agent tree on its
/// own rows, so a team lead sits above the agents it started.
pub fn place_teams(s: &Snapshot, area: Rect, node_w: u16, node_h: u16) -> (Vec<Node>, Vec<Group>) {
    let mut nodes = Vec::new();
    let mut groups = Vec::new();
    let coords: Vec<&crate::model::Agent> = s.agents.iter().filter(|a| a.coordinator).collect();
    let top_h = if coords.is_empty() { 0 } else { node_h + 2 };
    if !coords.is_empty() {
        let slot = area.width / coords.len() as u16;
        for (k, a) in coords.iter().enumerate() {
            nodes.push(Node {
                id: a.id.clone(),
                x: area.x + slot * k as u16 + slot / 2,
                y: area.y + top_h / 2,
            });
        }
    }
    // Teams with their members, busiest first.
    let mut teams: Vec<(String, Vec<&crate::model::Agent>)> = Vec::new();
    for a in s.agents.iter().filter(|a| !a.coordinator) {
        let k = a.team_key();
        match teams.iter_mut().find(|(t, _)| *t == k) {
            Some((_, v)) => v.push(a),
            None => teams.push((k, vec![a])),
        }
    }
    teams.sort_by_key(|(_, m)| std::cmp::Reverse(m.len()));
    if teams.is_empty() {
        return (nodes, groups);
    }
    let body = Rect {
        x: area.x,
        y: area.y + top_h,
        width: area.width,
        height: area.height.saturating_sub(top_h),
    };
    let max_cols = (body.width / (node_w + 4)).max(1) as usize;
    let cols = teams.len().min(max_cols).max(1);
    let rows = teams.len().div_ceil(cols);
    let cell_w = body.width / cols as u16;
    let cell_h = body.height / rows.max(1) as u16;
    for (ti, (team, members)) in teams.into_iter().enumerate() {
        let (r, c) = (ti / cols, ti % cols);
        let cluster = Rect {
            x: body.x + cell_w * c as u16,
            y: body.y + cell_h * r as u16,
            width: cell_w,
            height: cell_h,
        };
        groups.push(Group {
            host: team,
            area: cluster,
        });
        // Depth inside the team: leads (parent outside the team) on the first row.
        let ids: Vec<&str> = members.iter().map(|a| a.id.as_str()).collect();
        let local = |a: &crate::model::Agent| {
            let mut d = 0;
            let mut p = a.parent.as_deref();
            while let Some(pid) = p {
                if !ids.contains(&pid) {
                    break;
                }
                d += 1;
                p = s.agent(pid).and_then(|x| x.parent.as_deref());
            }
            d
        };
        let per_row = ((cluster.width.saturating_sub(2)) / (node_w + 1)).max(1) as usize;
        let max_depth = members.iter().map(|a| local(a)).max().unwrap_or(0);
        let mut rows_of: Vec<Vec<&crate::model::Agent>> = Vec::new();
        for d in 0..=max_depth {
            let level: Vec<_> = members.iter().copied().filter(|a| local(a) == d).collect();
            for chunk in level.chunks(per_row) {
                rows_of.push(chunk.to_vec());
            }
        }
        let n_rows = rows_of.len().max(1) as u16;
        let row_h = (cluster.height.saturating_sub(2) / n_rows).max(node_h);
        for (ri, row) in rows_of.into_iter().enumerate() {
            let y = cluster.y + 1 + row_h * ri as u16 + row_h / 2;
            let slot = cluster.width / row.len().max(1) as u16;
            for (k, a) in row.into_iter().enumerate() {
                let x = (cluster.x + slot * k as u16 + slot / 2).clamp(
                    cluster.x + node_w / 2 + 1,
                    (cluster.x + cluster.width)
                        .saturating_sub(node_w / 2 + 1)
                        .max(cluster.x + node_w / 2 + 1),
                );
                nodes.push(Node {
                    id: a.id.clone(),
                    x,
                    y: y.max(cluster.y + node_h / 2 + 1),
                });
            }
        }
    }
    (nodes, groups)
}

/// The rectangle of a node box centered on `n`, clipped to `bounds`.
pub fn node_rect(n: &Node, w: u16, h: u16, bounds: Rect) -> Rect {
    let x = n.x.saturating_sub(w / 2).max(bounds.x);
    let y = n.y.saturating_sub(h / 2).max(bounds.y);
    let w = w.min((bounds.x + bounds.width).saturating_sub(x));
    let h = h.min((bounds.y + bounds.height).saturating_sub(y));
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

/// Points on the straight line from `a` to `b` (Bresenham), in cells.
pub fn line_cells(a: (u16, u16), b: (u16, u16)) -> Vec<(u16, u16)> {
    let (mut x0, mut y0) = (a.0 as i32, a.1 as i32);
    let (x1, y1) = (b.0 as i32, b.1 as i32);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut out = Vec::new();
    loop {
        out.push((x0 as u16, y0 as u16));
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
    out
}

/// Position `t` (0–1) of the way from `a` to `b`.
pub fn lerp(a: (u16, u16), b: (u16, u16), t: f64) -> (u16, u16) {
    let t = t.clamp(0.0, 1.0);
    let x = a.0 as f64 + (b.0 as f64 - a.0 as f64) * t;
    let y = a.1 as f64 + (b.1 as f64 - a.1 as f64) * t;
    (x.round() as u16, y.round() as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_includes_endpoints() {
        let l = line_cells((0, 0), (4, 2));
        assert_eq!(l.first(), Some(&(0, 0)));
        assert_eq!(l.last(), Some(&(4, 2)));
        assert_eq!(l.len(), 5);
    }

    #[test]
    fn lerp_midpoint() {
        assert_eq!(lerp((0, 0), (10, 4), 0.5), (5, 2));
    }
}
