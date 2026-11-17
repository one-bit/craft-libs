//! A key press: along a page axis to the first target strictly ahead, within `reach`, else by the fallback.
//!
//! For each side, direction and kind the nearest line strictly ahead is a candidate: past the tolerance, so a box already on a line moves off
//! it. It is found by an upper-bound bisection in the sorted array, so "however far" costs what near does. A line in one of a turned box's own
//! directions is ahead at the travel `(c - side) / (n · e)` along the axis `e`. The next lattice line, and the next equal-gap position in the
//! row, are ahead too: a nudge stops at every kind a move snaps to. The smallest travel wins, with priority on ties. The reach of a kind does not
//! apply, since the nudge's own reach bounds the travel.

use kurbo::Vec2;

use crate::align::lines_ahead;
use crate::budget::Refusal;
use crate::ctx::{Cand, Ctx};
use crate::frame::{Anchors, Axis, Side, TurnedBox};
use crate::kind::Kind;
use crate::moves::box_probes;
use crate::query::NudgeReach;
use crate::solve::{HitRec, Solution};

/// Whether travel `a` is a better stop than `b`: the shorter, with priority, then id, on a tie within the tolerance.
fn nearer(cx: &Ctx<'_>, a: &Cand, b: &Cand) -> bool {
    let by_len = if cx.co.same(a.t, b.t) { std::cmp::Ordering::Equal } else { a.t.total_cmp(&b.t) };
    by_len.then(a.prio.cmp(&b.prio)).then(a.id.cmp(&b.id)).then(a.side.cmp(&b.side)).then(a.kind.cmp(&b.kind)).is_lt()
}

/// Nudges a box along a page axis to the first target ahead, or by the fallback.
pub(crate) fn solve_nudge(cx: &mut Ctx<'_>, frame: TurnedBox, axis: Axis, forward: bool, reach: NudgeReach, fallback: f64) -> Solution {
    let e: Vec2 = axis.unit() * if forward { 1.0 } else { -1.0 };
    cx.region = cx.t.rel_bounds(&frame);
    let limit = match reach {
        NudgeReach::Within(step) => step,
        NudgeReach::Anywhere => f64::INFINITY,
    };
    let mut cands: Vec<Cand> = Vec::new();
    let probes = box_probes(cx, &frame, Anchors::ALL, Anchors::ALL);
    for p in &probes {
        let ne = p.normal.dot(e);
        if ne.abs() < 1.0e-12 {
            continue;
        }
        for ps in &p.sides {
            for f in lines_ahead(cx, p.angle, ps.offset, ps.side, ne > 0.0, if limit.is_finite() { limit } else { 0.0 }) {
                let travel = (f.c - ps.offset) / ne;
                if !(travel.is_finite() && travel > 0.0 && travel <= limit + cx.co.tol(travel, limit.min(1.0e300))) {
                    continue;
                }
                cands.push(Cand {
                    kind: f.kind,
                    id: f.id,
                    prio: cx.rule(f.kind).priority,
                    reach: limit,
                    side: ps.side,
                    own: ps.own,
                    angle: p.angle,
                    c: f.c,
                    s: ps.offset,
                    d: travel,
                    t: travel,
                    pt: 0,
                });
            }
        }
    }
    // The next place that repeats a gap in the row along the axis.
    let row_angle = if axis == Axis::X { 0.0 } else { std::f64::consts::FRAC_PI_2 };
    // (only where the subject aligns along that page axis at all: with the page's directions off, a turned box has no row along it)
    let page_row =
        probes.iter().any(|p| p.sides.iter().any(|s| s.own == crate::frame::OwnOrBounds::Bounds) && (p.normal - axis.unit()).hypot() < 1.0e-9);
    let rows = if page_row { crate::spacing::ahead(cx, &frame, axis.unit(), forward) } else { Vec::new() };
    for (travel, nb) in rows {
        if travel <= limit + cx.co.tol(travel, limit.min(1.0e300)) {
            let prio = cx.rule(Kind::Spacing).priority;
            cands.push(Cand {
                kind: Kind::Spacing,
                id: Some(nb.id),
                prio,
                reach: limit,
                side: Side::Min,
                own: crate::frame::OwnOrBounds::Bounds,
                angle: row_angle,
                c: 0.0,
                s: 0.0,
                d: travel,
                t: travel,
                pt: 0,
            });
        }
    }
    // The nearest stop wins; a gap repeated in the row as it was is checked again in the row as it will be, which may be another.
    let mut best: Option<Cand> = None;
    while let Some(i) = (0..cands.len()).reduce(|b, i| if cands.get(i).zip(cands.get(b)).is_some_and(|(c, bb)| nearer(cx, c, bb)) { i } else { b }) {
        let c = cands.swap_remove(i);
        let holds = c.kind != Kind::Spacing || {
            let to = TurnedBox { centre: frame.centre + e * c.t, ..frame };
            let probe = crate::ctx::Probe { angle: row_angle, normal: axis.unit(), sides: Vec::new() };
            crate::spacing::still_holds(cx, &to, &probe)
        };
        if holds {
            best = Some(c);
            break;
        }
    }
    let travel = best.map_or(fallback, |b| b.t);
    let mut sol = Solution::new();
    if !travel.is_finite() {
        cx.work.refused = Some(Refusal::NotFinite);
        return sol;
    }
    sol.k = e * travel;
    sol.frame = Some(TurnedBox { centre: frame.centre + sol.k, ..frame });
    sol.hits.extend(best.map(HitRec::Line));
    sol
}
