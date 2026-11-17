//! The query: candidates per free degree of the subject, inside the constraint and the rounding, chosen by the settings; then the marks, derived
//! from the final geometry.

use kurbo::{Point, Vec2};

use crate::budget::{Refusal, Work};
use crate::ctx::{Cand, Ctx};
use crate::frame::{Axis, Direction, TurnedBox};
use crate::marks;
use crate::moves;
use crate::query::{Constraint, Detail, Query, Rounding, Subject};
use crate::resize;
use crate::result::{Hit, Snapped};
use crate::targets::Targets;

/// What a degree snapped to, with what the marks pass needs to know of it.
#[derive(Clone, Debug)]
pub(crate) enum HitRec {
    /// A side of the subject on a line.
    Line(Cand),
    /// A width or height of the subject equal to another object's.
    Size(Cand),
    /// An angle equal to another object's, or on a lattice of angles.
    Angle(crate::angle::AngleHit),
    /// A place of the subject on a target place.
    Place(crate::places::PlaceCand),
}

/// Where the subject lands and what it snapped to.
#[derive(Clone, Debug)]
pub(crate) struct Solution {
    pub k: Vec2,
    pub frame: Option<TurnedBox>,
    pub point: Option<Point>,
    pub angle: Option<f64>,
    pub hits: Vec<HitRec>,
}

impl Solution {
    pub fn new() -> Solution {
        Solution { k: Vec2::ZERO, frame: None, point: None, angle: None, hits: Vec::new() }
    }
}

fn refusal(q: &Query<'_>) -> Option<Refusal> {
    if !(q.zoom.is_finite() && q.zoom > 0.0) {
        return Some(Refusal::Zoom);
    }
    if !q.settings.is_usable() {
        return Some(Refusal::Settings);
    }
    let subject_ok = match q.subject {
        Subject::Point { at, base } => at.is_finite() && base.is_none_or(|b| b.is_finite()),
        Subject::Box { frame, own, .. } => frame.is_valid() && own.iter().all(|p| p.is_finite()),
        Subject::Edges { frame, .. } => frame.is_valid(),
        Subject::Rotate { pivot, angle, radius, step } => {
            pivot.is_finite() && angle.is_finite() && radius.is_finite() && step.is_none_or(|(s, z)| s.is_finite() && s > 0.0 && z.is_finite())
        }
        Subject::Nudge { frame, step, .. } => frame.is_valid() && step.is_finite() && step >= 0.0,
    };
    let constraint_ok = match q.constraint {
        Constraint::None | Constraint::Centre | Constraint::Axis(_) => true,
        Constraint::Ray { origin, dir } => origin.is_finite() && dir.is_finite() && dir.hypot() > 0.0,
        Constraint::Aspect { ratio, .. } => ratio.is_finite() && ratio > 0.0,
        Constraint::Angles { step, zero } => step.is_finite() && step > 0.0 && zero.is_finite(),
    };
    let rounding_ok = match q.rounding {
        Rounding::None => true,
        Rounding::Delta { step, from } => from.is_finite() && !step.x.is_nan() && !step.y.is_nan(),
        Rounding::Position(step) => !step.x.is_nan() && !step.y.is_nan(),
    };
    (!(subject_ok && constraint_ok && rounding_ok)).then_some(Refusal::NotFinite)
}

/// Asks the targets where the subject of `query` comes to rest, and what to draw about it.
pub fn snap(targets: &Targets, query: &Query<'_>, detail: &mut dyn Detail) -> Snapped {
    let mut cx = Ctx::new(targets, query);
    if let Some(why) = refusal(query) {
        cx.work.refused = Some(why);
        return Snapped::empty(cx.work);
    }
    prefetch_bounds(&mut cx, detail);
    let sol = match query.subject {
        Subject::Point { at, base } => moves::solve_point(&mut cx, detail, at, base),
        Subject::Box { frame, u, v, own } => moves::solve_box(&mut cx, detail, frame, u, v, own),
        Subject::Edges { frame, moving } => resize::solve_edges(&mut cx, detail, frame, moving),
        Subject::Rotate { angle, radius, step, .. } => crate::angle::solve_rotate(&mut cx, angle, radius, step),
        Subject::Nudge { frame, axis, forward, step } => crate::nudge::solve_nudge(&mut cx, frame, axis, forward, step),
    };
    let marks = marks::derive(&mut cx, &sol);
    finish(cx.work, sol, marks)
}

/// With `BoxRound::Paths`, asks the detail source for the box round the outline of each turned object near the subject, once, charged to the
/// outlines budget. An object it has none for keeps the box round its corners (or the one the app gave the builder).
fn prefetch_bounds(cx: &mut Ctx<'_>, detail: &mut dyn Detail) {
    if cx.s.box_round != crate::settings::BoxRound::Paths {
        return;
    }
    let frame = match cx.q.subject {
        Subject::Box { frame, .. } | Subject::Edges { frame, .. } | Subject::Nudge { frame, .. } => frame,
        _ => return,
    };
    let region = cx.t.rel_bounds(&frame);
    let r = cx.s.spacing_reach;
    let along = (r.times * frame.size.width.max(frame.size.height)).max(r.at_least);
    let across = cx.s.align_reach.map_or(0.0, |a| a.document(cx.zoom)).min(1.0e300);
    let reach =
        along.max(if across.is_finite() { across } else { 0.0 }).max(cx.reach(crate::kind::Kind::Edge)).max(cx.reach(crate::kind::Kind::Centre));
    let window = crate::ctx::grown(region, reach);
    let t = cx.t;
    let (limit, visits) = (u64::from(cx.budget().candidates) as usize, u64::from(cx.budget().window_visits));
    let g = t.object_grid(&mut cx.work.made).gather(&window, limit, visits);
    cx.work.examined = cx.work.examined.saturating_add(g.examined);
    cx.work.visited = cx.work.visited.saturating_add(g.visited);
    let cap = u64::from(cx.budget().outlines);
    let mut got: Vec<(u32, kurbo::Rect)> = Vec::new();
    for c in &g.items {
        let Some(o) = t.objects.get(c.index as usize) else { continue };
        if !o.parts.detail || o.frame.quarter(cx.co.angle).is_some() {
            continue;
        }
        if !crate::budget::Work::charge(&mut cx.work.outlines, cap, 1, &mut cx.work.cut, &mut cx.work.truncated, crate::budget::Cuts::OUTLINES) {
            break;
        }
        let q = cx.q;
        let found = {
            let mut spend = crate::budget::Spend { work: &mut cx.work, budget: &q.budget };
            detail.bounds(o.id, &mut spend)
        };
        if let Some(b) = found.filter(|b| [b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite())) {
            got.push((c.index, b));
        }
    }
    got.sort_by_key(|p| p.0);
    cx.path_bounds = got;
}

fn finish(mut work: Work, sol: Solution, marks: Vec<crate::result::Mark>) -> Snapped {
    let hits = sol.hits.iter().map(hit_of).collect();
    work.marks = marks.len() as u64;
    Snapped { delta: sol.k, frame: sol.frame, point: sol.point, angle: sol.angle, hits, marks, work }
}

fn hit_of(h: &HitRec) -> Hit {
    match h {
        HitRec::Place(p) => Hit { kind: p.g.kind, id: p.g.id, direction: None, side: None, number: p.g.number, distance: p.dist },
        HitRec::Angle(a) => Hit { kind: a.kind, id: a.id, direction: None, side: None, number: None, distance: a.delta.abs() },
        HitRec::Size(c) => Hit { kind: c.kind, id: c.id, direction: None, side: Some((c.side, c.own)), number: None, distance: c.t },
        HitRec::Line(c) => {
            Hit { kind: c.kind, id: c.id, direction: Direction::new(c.angle), side: Some((c.side, c.own)), number: None, distance: c.t }
        }
    }
}

/// Which page axes a rounding may change.
#[derive(Clone, Copy, Debug)]
pub(crate) enum RoundOn {
    Both,
    Only(Axis),
    /// Along a ray: the axis that leads is rounded and the move along the ray follows.
    Lead(Vec2),
}

fn round_step(v: f64, step: f64) -> Option<f64> {
    (step.is_finite() && step > 0.0).then(|| (v / step).round() * step).filter(|r| r.is_finite())
}

/// The correction `k` after the rounding. `refp` is the subject's reference place and `minp` the min corner of the box round it, both before
/// `k` is added; `base` is a move already applied to both (the projection onto a ray).
pub(crate) fn round_k(rounding: &Rounding, refp: Point, minp: Point, k: Vec2, on: RoundOn) -> Vec2 {
    let axes: [(Axis, bool); 2] = match on {
        RoundOn::Both => [(Axis::X, true), (Axis::Y, true)],
        RoundOn::Only(a) => [(Axis::X, a == Axis::X), (Axis::Y, a == Axis::Y)],
        RoundOn::Lead(e) => {
            let lead = if e.x.abs() >= e.y.abs() { Axis::X } else { Axis::Y };
            [(Axis::X, lead == Axis::X), (Axis::Y, lead == Axis::Y)]
        }
    };
    let one = |axis: Axis, v: f64| -> f64 {
        let (r, m, s, from) = match axis {
            Axis::X => (refp.x, minp.x, step_of(rounding).x, from_of(rounding).x),
            Axis::Y => (refp.y, minp.y, step_of(rounding).y, from_of(rounding).y),
        };
        match rounding {
            Rounding::None => v,
            Rounding::Delta { .. } => round_step(r + v - from, s).map_or(v, |moved| from + moved - r),
            Rounding::Position(_) => round_step(m + v, s).map_or(v, |p| p - m),
        }
    };
    let mut out = k;
    for (axis, on) in axes {
        if !on {
            continue;
        }
        match axis {
            Axis::X => out.x = one(axis, k.x),
            Axis::Y => out.y = one(axis, k.y),
        }
    }
    if let RoundOn::Lead(e) = on {
        // The other axis follows along the ray: the same fraction of the ray's direction.
        let lead = if e.x.abs() >= e.y.abs() { Axis::X } else { Axis::Y };
        let (lead_v, e_lead, k_lead) = match lead {
            Axis::X => (out.x, e.x, k.x),
            Axis::Y => (out.y, e.y, k.y),
        };
        if e_lead.abs() > 1.0e-12 && k_lead != lead_v {
            let t = (lead_v - k_lead) / e_lead;
            let moved = k + e * t;
            out = moved;
        }
    }
    out
}

fn step_of(r: &Rounding) -> Vec2 {
    match r {
        Rounding::Delta { step, .. } | Rounding::Position(step) => *step,
        Rounding::None => Vec2::ZERO,
    }
}

fn from_of(r: &Rounding) -> Point {
    match r {
        Rounding::Delta { from, .. } => *from,
        _ => Point::ORIGIN,
    }
}
