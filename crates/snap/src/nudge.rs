//! A key press: along a page axis to the first target strictly ahead within one step, else by the step.
//!
//! For each side, direction and kind the nearest line strictly ahead is a candidate: past the tolerance, so a box already on a line moves off
//! it, and at most one step away. A line in one of a turned box's own directions is ahead at the travel `(c - side) / (n · e)` along the axis
//! `e`. The next lattice line is ahead too. The smallest travel wins, with priority on ties; the reach does not apply, since the step bounds the
//! travel.

use kurbo::Vec2;

use crate::align::lines_ahead;
use crate::ctx::{Cand, Ctx};
use crate::frame::{Anchors, Axis, TurnedBox};
use crate::moves::box_probes;
use crate::solve::{HitRec, Solution};

/// Nudges a box by one step along a page axis, or to the first target ahead.
pub(crate) fn solve_nudge(cx: &mut Ctx<'_>, frame: TurnedBox, axis: Axis, forward: bool, step: f64) -> Solution {
    let e: Vec2 = axis.unit() * if forward { 1.0 } else { -1.0 };
    cx.region = cx.t.rel_bounds(&frame);
    let probes = box_probes(cx, &frame, Anchors::ALL, Anchors::ALL);
    let mut best: Option<Cand> = None;
    for p in &probes {
        let ne = p.normal.dot(e);
        if ne.abs() < 1.0e-12 {
            continue;
        }
        for ps in &p.sides {
            for f in lines_ahead(cx, p.angle, ps.offset, ps.side, ne > 0.0, step) {
                let travel = (f.c - ps.offset) / ne;
                if !(travel.is_finite() && travel > 0.0 && travel <= step + cx.co.tol(travel, step)) {
                    continue;
                }
                let c = Cand {
                    kind: f.kind,
                    id: f.id,
                    prio: cx.rule(f.kind).priority,
                    reach: step,
                    side: ps.side,
                    own: ps.own,
                    angle: p.angle,
                    c: f.c,
                    s: ps.offset,
                    d: travel,
                    t: travel,
                    rate: ne,
                    pt: 0,
                    count: f.count,
                };
                let better = best.is_none_or(|b| {
                    let by_len = if cx.co.same(c.t, b.t) { std::cmp::Ordering::Equal } else { c.t.total_cmp(&b.t) };
                    by_len.then(c.prio.cmp(&b.prio)).then(c.id.cmp(&b.id)).then(c.side.cmp(&b.side)).then(c.kind.cmp(&b.kind)).is_lt()
                });
                if better {
                    best = Some(c);
                }
            }
        }
    }
    let travel = best.map_or(step, |b| b.t);
    let mut sol = Solution::new();
    sol.k = e * travel;
    sol.frame = Some(TurnedBox { centre: frame.centre + sol.k, ..frame });
    sol.hits.extend(best.map(HitRec::Line));
    sol
}
