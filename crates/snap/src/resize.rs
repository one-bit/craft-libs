// Resize along a box's own axes follows VectorCraft's `snap_scale`: storytold/vectorcraft at 9a01d266b0438f96608c9f75b51437f912587d22,
// crates/tools/src/guides/targets.rs. Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
// Size match follows DesignCraft's `pass_dimensions` and `length_edge_delta`: storytold/designcraft at 47ef1ec411d53e98a31c576d80a1d0e20ef5eb04,
// crates/tools/src/snap.rs. Copyright (c) 2026 ArtCraft Team and the DesignCraft contributors. MIT OR Apache-2.0.
// Changed: every moving side, half-size and scale is one degree of a general solve, and a candidate is checked on the final extents.

//! A resize, or the dragged corner of a draw: sides that move along the box's own axes, in a turned frame as well as the page's.
//!
//! Each free degree is one parameter `g`: the travel of a moving side, the growth of a half-size when opposite sides move together, or the
//! scale about a fixed corner when the proportions are kept. Every point a degree carries (the two ends and the middle of each moving side)
//! moves linearly with `g`, so a line `n · p = c` is met at `g = (c - n · p0) / (n · w)`. A line in the side's own direction pulls it
//! directly; a line in another direction pulls it through the points it carries. A size match is the same with a length for a line. A
//! candidate that would collapse or flip the box is not offered. (The one-parameter search for a side or a proportional corner, the travel
//! limit, and the collapse and flip refusals follow VectorCraft's `snap_scale`; see the README.)

use kurbo::{Point, Vec2};

use crate::align::lines_near;
use crate::ctx::{Cand, Ctx, Probe, ProbeSide, top};
use crate::frame::{OwnOrBounds, Side, TurnedBox};
use crate::kind::Kind;
use crate::query::{About, Constraint, Detail, Edges, Rounding};
use crate::settings::SizeAxes;
use crate::solve::{HitRec, Solution};

/// Own-axis extents of the box, relative to the origin: `u` from `a0` to `a1`, `v` from `b0` to `b1`.
#[derive(Clone, Copy, Debug)]
struct Ext {
    a0: f64,
    a1: f64,
    b0: f64,
    b1: f64,
}

impl Ext {
    /// Whether it is a box with some size on both axes: more than the same-place rule can tell from nothing, for a box that has gone to nothing
    /// by its own arithmetic is left a few millionths of a unit wide far from the origin.
    fn sized(&self, co: &crate::frame::Coincide) -> bool {
        self.a1 - self.a0 > co.tol(self.a1, self.a0) && self.b1 - self.b0 > co.tol(self.b1, self.b0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SideId {
    UMin,
    UMax,
    VMin,
    VMax,
}

#[derive(Clone, Copy, Debug)]
enum DegKind {
    /// One moving side travels `g` along its own axis.
    Side(SideId),
    /// Opposite sides move together: the half-size along `u` (true) or `v` grows by `g`.
    Half(bool),
    /// The box scales by `1 + g` about the point `(fa, fb)` in own coordinates.
    Scale(f64, f64),
}

impl DegKind {
    fn apply(&self, e: Ext, g: f64) -> Ext {
        match *self {
            DegKind::Side(SideId::UMin) => Ext { a0: e.a0 + g, ..e },
            DegKind::Side(SideId::UMax) => Ext { a1: e.a1 + g, ..e },
            DegKind::Side(SideId::VMin) => Ext { b0: e.b0 + g, ..e },
            DegKind::Side(SideId::VMax) => Ext { b1: e.b1 + g, ..e },
            DegKind::Half(true) => Ext { a0: e.a0 - g, a1: e.a1 + g, ..e },
            DegKind::Half(false) => Ext { b0: e.b0 - g, b1: e.b1 + g, ..e },
            DegKind::Scale(fa, fb) => {
                let s = 1.0 + g;
                Ext { a0: fa + s * (e.a0 - fa), a1: fa + s * (e.a1 - fa), b0: fb + s * (e.b0 - fb), b1: fb + s * (e.b1 - fb) }
            }
        }
    }

    /// Whether the extents it gives are a box with some size on both axes: more than the same-place rule can tell from nothing.
    fn valid(&self, co: &crate::frame::Coincide, e: Ext, g: f64) -> bool {
        g.is_finite() && self.apply(e, g).sized(co)
    }
}

/// A degree: how it moves, and the points it carries with the way each moves for a unit of `g`.
struct Degree {
    kind: DegKind,
    carried: Vec<(Vec2, Vec2, SideId, u8)>,
}

/// The geometry of a frame in its own axes.
struct Own {
    u: Vec2,
    v: Vec2,
    ext: Ext,
}

impl Own {
    fn of(frame: &TurnedBox, o: Point) -> Own {
        let (u, v) = (frame.u(), frame.v());
        let c = (frame.centre - o).to_point().to_vec2();
        let (cu, cv) = (u.dot(c), v.dot(c));
        let (hw, hh) = (0.5 * frame.size.width, 0.5 * frame.size.height);
        Own { u, v, ext: Ext { a0: cu - hw, a1: cu + hw, b0: cv - hh, b1: cv + hh } }
    }

    fn at(&self, a: f64, b: f64) -> Vec2 {
        self.u * a + self.v * b
    }

    /// The two ends and the middle of a side.
    fn side_points(&self, id: SideId, e: &Ext) -> [Vec2; 3] {
        let (mb, ma) = (0.5 * (e.b0 + e.b1), 0.5 * (e.a0 + e.a1));
        match id {
            SideId::UMin => [self.at(e.a0, e.b0), self.at(e.a0, mb), self.at(e.a0, e.b1)],
            SideId::UMax => [self.at(e.a1, e.b0), self.at(e.a1, mb), self.at(e.a1, e.b1)],
            SideId::VMin => [self.at(e.a0, e.b0), self.at(ma, e.b0), self.at(e.a1, e.b0)],
            SideId::VMax => [self.at(e.a0, e.b1), self.at(ma, e.b1), self.at(e.a1, e.b1)],
        }
    }

    fn axis_of(&self, id: SideId) -> Vec2 {
        if matches!(id, SideId::UMin | SideId::UMax) { self.u } else { self.v }
    }

    fn rebuild(&self, e: Ext, o: Point, frame: &TurnedBox) -> TurnedBox {
        let c = self.at(0.5 * (e.a0 + e.a1), 0.5 * (e.b0 + e.b1));
        TurnedBox { centre: o + c, size: kurbo::Size::new(e.a1 - e.a0, e.b1 - e.b0), angle: frame.angle }
    }
}

fn flagged(m: &Edges) -> Vec<SideId> {
    [(m.u_min, SideId::UMin), (m.u_max, SideId::UMax), (m.v_min, SideId::VMin), (m.v_max, SideId::VMax)]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, id)| id)
        .collect()
}

/// The sides whose points are lined up: the moving ones, and with the sides moving together, both of each axis that has one.
fn carrying(cx: &Ctx<'_>, moving: &Edges) -> Vec<SideId> {
    let sides = flagged(moving);
    if !matches!(cx.q.constraint, Constraint::Centre) {
        return sides;
    }
    let both = |a: SideId, b: SideId| sides.contains(&a) || sides.contains(&b);
    let mut out = Vec::new();
    if both(SideId::UMin, SideId::UMax) {
        out.extend([SideId::UMin, SideId::UMax]);
    }
    if both(SideId::VMin, SideId::VMax) {
        out.extend([SideId::VMin, SideId::VMax]);
    }
    out
}

/// The degrees the constraint leaves the moving sides.
fn degrees(cx: &Ctx<'_>, own: &Own, moving: &Edges) -> Vec<Degree> {
    let sides = flagged(moving);
    let e = own.ext;
    let carry = |ids: &[SideId], w: &dyn Fn(SideId, Vec2) -> Vec2| -> Vec<(Vec2, Vec2, SideId, u8)> {
        ids.iter()
            .flat_map(|id| own.side_points(*id, &e).into_iter().enumerate().map(move |(i, p)| (p, *id, i as u8)))
            .map(|(p, id, i)| (p, w(id, p), id, i))
            .collect()
    };
    match cx.q.constraint {
        Constraint::Centre => {
            let mut out = Vec::new();
            for (is_u, ids) in [(true, [SideId::UMin, SideId::UMax]), (false, [SideId::VMin, SideId::VMax])] {
                if ids.iter().any(|i| sides.contains(i)) {
                    let carried = carry(&ids, &|id, _| if matches!(id, SideId::UMax | SideId::VMax) { own.axis_of(id) } else { -own.axis_of(id) });
                    out.push(Degree { kind: DegKind::Half(is_u), carried });
                }
            }
            out
        }
        Constraint::Aspect { about, .. } => {
            if sides.is_empty() {
                return Vec::new();
            }
            let fixed = |lo: SideId, hi: SideId, c0: f64, c1: f64| -> f64 {
                match (about, sides.contains(&lo), sides.contains(&hi)) {
                    (About::Fixed, true, false) => c1,
                    (About::Fixed, false, true) => c0,
                    _ => 0.5 * (c0 + c1),
                }
            };
            let (fa, fb) = (fixed(SideId::UMin, SideId::UMax, e.a0, e.a1), fixed(SideId::VMin, SideId::VMax, e.b0, e.b1));
            let f = own.at(fa, fb);
            let carried = carry(&sides, &|_, p| p - f);
            vec![Degree { kind: DegKind::Scale(fa, fb), carried }]
        }
        _ => sides.iter().map(|id| Degree { kind: DegKind::Side(*id), carried: carry(&[*id], &|id, _| own.axis_of(id)) }).collect(),
    }
}

/// The directions a moving side's points are lined up along: the page's, and the box's own when it is turned.
fn directions(cx: &Ctx<'_>, frame: &TurnedBox) -> Vec<(f64, OwnOrBounds)> {
    let quarter = frame.quarter(cx.co.angle).is_some();
    let mut out = Vec::new();
    if cx.s.frames.page || (quarter && cx.s.frames.own) {
        out.extend([(0.0, OwnOrBounds::Bounds), (std::f64::consts::FRAC_PI_2, OwnOrBounds::Bounds)]);
    }
    if cx.s.frames.own && !quarter {
        out.extend([(frame.angle, OwnOrBounds::Own), (frame.angle + std::f64::consts::FRAC_PI_2, OwnOrBounds::Own)]);
    }
    out
}

fn side_code(id: SideId) -> u8 {
    match id {
        SideId::UMin => 0,
        SideId::UMax => 1,
        SideId::VMin => 2,
        SideId::VMax => 3,
    }
}

fn side_of_code(code: u8) -> SideId {
    match code {
        0 => SideId::UMin,
        1 => SideId::UMax,
        2 => SideId::VMin,
        _ => SideId::VMax,
    }
}

fn side_label(id: SideId) -> Side {
    if matches!(id, SideId::UMin | SideId::VMin) { Side::Min } else { Side::Max }
}

/// The candidates of one degree: each carried point against the lines of each direction, and the lengths that a size match offers.
fn degree_cands(cx: &mut Ctx<'_>, own: &Own, deg: &Degree, dirs: &[(f64, OwnOrBounds)]) -> Vec<Cand> {
    let mut out = Vec::new();
    for (p0, w, id, idx) in &deg.carried {
        for (angle, family) in dirs {
            let (a, flipped) = crate::index::canon(*angle, cx.co.angle);
            let label = crate::frame::canon_side(side_label(*id), flipped);
            let n = crate::frame::unit(a);
            let nw = n.dot(*w);
            if nw.abs() < 1.0e-12 {
                continue;
            }
            let s = n.dot(*p0);
            for f in lines_near(cx, a, s, label) {
                let g = (f.c - s) / nw;
                let travel = g.abs() * w.hypot();
                if !g.is_finite() || !cx.pulls(f.kind, travel) || !deg.kind.valid(&cx.co, own.ext, g) {
                    continue;
                }
                out.push(Cand {
                    kind: f.kind,
                    id: f.id,
                    prio: cx.rule(f.kind).priority,
                    reach: cx.reach(f.kind),
                    side: label,
                    own: *family,
                    angle: a,
                    c: f.c,
                    s,
                    d: g,
                    t: travel,
                    pt: side_code(*id) * 4 + idx,
                });
            }
        }
    }
    out.extend(size_cands(cx, own, deg));
    out
}

/// The length in a sorted array nearest to `len`, within one bisection: one of the two beside its insertion point.
fn nearest_length(arr: &[crate::index::Length], len: f64) -> Option<crate::index::Length> {
    let i = arr.partition_point(|l| l.length < len);
    [i.checked_sub(1), Some(i)]
        .into_iter()
        .flatten()
        .filter_map(|j| arr.get(j))
        .min_by(|a, b| (a.length - len).abs().total_cmp(&(b.length - len).abs()).then(a.id.cmp(&b.id)))
        .copied()
}

/// The size candidates of a degree: other objects' own widths and heights, by bisection within the reach.
fn size_cands(cx: &mut Ctx<'_>, own: &Own, deg: &Degree) -> Vec<Cand> {
    let rule = cx.rule(Kind::Size);
    if !rule.on {
        return Vec::new();
    }
    let e = own.ext;
    let (w, h) = (e.a1 - e.a0, e.b1 - e.b0);
    // For each axis the degree changes: its current length, the rate at which the length changes with `g`, and the corner travel per `g`.
    let changes: Vec<(bool, f64, f64, f64)> = match deg.kind {
        DegKind::Side(SideId::UMin) => vec![(true, w, -1.0, 1.0)],
        DegKind::Side(SideId::UMax) => vec![(true, w, 1.0, 1.0)],
        DegKind::Side(SideId::VMin) => vec![(false, h, -1.0, 1.0)],
        DegKind::Side(SideId::VMax) => vec![(false, h, 1.0, 1.0)],
        DegKind::Half(is_u) => vec![if is_u { (true, w, 2.0, 1.0) } else { (false, h, 2.0, 1.0) }],
        DegKind::Scale(..) => {
            let travel = deg.carried.iter().map(|(_, w, _, _)| w.hypot()).fold(0.0, f64::max);
            vec![(true, w, w, travel), (false, h, h, travel)]
        }
    };
    let (widths, heights) = cx.lengths();
    let mut out = Vec::new();
    for (is_w, len, rate, per_g) in changes {
        let arrays: Vec<&[crate::index::Length]> = match (cx.s.size_axes, is_w) {
            (SizeAxes::EitherAxis, _) => vec![widths, heights],
            (SizeAxes::SameAxis, true) => vec![widths],
            (SizeAxes::SameAxis, false) => vec![heights],
        };
        for arr in arrays {
            cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(crate::index::log_steps(arr.len()));
            let Some(m) = nearest_length(arr, len) else { continue };
            let g = (m.length - len) / rate;
            let travel = g.abs() * per_g;
            if !(g.is_finite() && cx.pulls(Kind::Size, travel) && deg.kind.valid(&cx.co, own.ext, g)) {
                continue;
            }
            out.push(Cand {
                kind: Kind::Size,
                id: Some(m.id),
                prio: rule.priority,
                reach: cx.reach(Kind::Size),
                side: Side::Mid,
                own: OwnOrBounds::Own,
                angle: if is_w { 0.0 } else { 1.0 },
                c: m.length,
                s: len,
                d: g,
                t: travel,
                pt: 0,
            });
        }
    }
    out
}

/// Whether the candidate still holds on the final extents: the point it was met by is on its line, or the length is the one matched.
fn holds(cx: &Ctx<'_>, own: &Own, c: &Cand, fin: &Ext) -> bool {
    if c.kind == Kind::Size {
        let len = if c.angle == 0.0 { fin.a1 - fin.a0 } else { fin.b1 - fin.b0 };
        return cx.co.same(len, c.c);
    }
    let points = own.side_points(side_of_code(c.pt / 4), fin);
    let Some(p) = points.get(usize::from(c.pt % 4)) else { return false };
    cx.co.same(crate::frame::unit(c.angle).dot(*p), c.c)
}

/// The point of a degree that leads the drag, as a point of a side (which side, and which of its three points), with the way it moves for a unit
/// of the degree: the middle of the moving side (the max side's when sides move together), or, when the proportions are kept and two sides move,
/// the dragged corner (the carried point that moves farthest).
fn leader(deg: &Degree) -> Option<(SideId, usize, Vec2)> {
    let middle = |max_only: bool| {
        deg.carried.iter().find(|(_, w, id, idx)| *idx == 1 && (!max_only || matches!(id, SideId::UMax | SideId::VMax)) && w.hypot() > 0.0)
    };
    let found = match deg.kind {
        DegKind::Side(_) => middle(false),
        DegKind::Half(_) => middle(true),
        DegKind::Scale(..) => {
            let mut sides: Vec<SideId> = Vec::new();
            for c in &deg.carried {
                if !sides.contains(&c.2) {
                    sides.push(c.2);
                }
            }
            if sides.len() == 1 { middle(false) } else { deg.carried.iter().max_by(|a, b| a.1.hypot().total_cmp(&b.1.hypot())) }
        }
    };
    found.map(|(_, w, id, idx)| (*id, usize::from(*idx), *w))
}

/// The values of the degrees once the rounding has been applied: the leading point of each degree lands, along the page axis it moves along most,
/// on the lattice (`Position`: of the step; `Delta`: a whole number of steps from the press), and the other coordinate follows the drag. This holds
/// for a turned box as for one square to the page, and for a kept aspect, where the leading axis rounds and the other follows the ratio. The leading
/// points of a turned box depend on each other (a corner drags two sides), so the values are solved together: each leading coordinate is a linear
/// function of the values, found by moving each by one, and the system is solved exactly. A rounding that cannot be solved, or that would collapse or
/// flip the box, is not made: the values stand as they were.
fn round_values(cx: &Ctx<'_>, own: &Own, degs: &[Degree], values: &[f64]) -> Vec<f64> {
    let (step, from) = match cx.q.rounding {
        Rounding::None => return values.to_vec(),
        Rounding::Position(step) => (step, None),
        Rounding::Delta { step, from } => (step, Some(from)),
    };
    // Which degrees have a leading point on an axis with a usable step, and which axis it is.
    let leads: Vec<Option<(SideId, usize, bool)>> = degs
        .iter()
        .map(|d| {
            let (id, idx, w) = leader(d)?;
            let x = w.x.abs() >= w.y.abs();
            let ok = (if x { w.x } else { w.y }).abs() > 1.0e-12
                && (if x { step.x } else { step.y }).is_finite()
                && (if x { step.x } else { step.y }) > 0.0;
            ok.then_some((id, idx, x))
        })
        .collect();
    let rows: Vec<usize> = leads.iter().enumerate().filter(|(_, l)| l.is_some()).map(|(i, _)| i).collect();
    if rows.is_empty() {
        return values.to_vec();
    }
    let at = |v: &[f64], row: usize| -> f64 {
        let Some((id, idx, x)) = leads.get(row).copied().flatten() else { return f64::NAN };
        let pts = own.side_points(id, &applied(degs, own, v));
        let p = pts.get(idx).copied().unwrap_or(Vec2::ZERO);
        if x { p.x + cx.o.x } else { p.y + cx.o.y }
    };
    let n = rows.len();
    let base: Vec<f64> = rows.iter().map(|r| at(values, *r)).collect();
    let want: Vec<f64> = rows
        .iter()
        .zip(&base)
        .map(|(r, b)| {
            let (st, f) = match leads.get(*r).copied().flatten() {
                Some((_, _, true)) => (step.x, from.map(|f| f.x)),
                _ => (step.y, from.map(|f| f.y)),
            };
            match f {
                Some(f) => f + ((b - f) / st).round() * st,
                None => (b / st).round() * st,
            }
        })
        .collect();
    // The matrix: how each leading coordinate moves when one degree's value moves by one.
    let mut m = vec![vec![0.0; n + 1]; n];
    for (c, dc) in rows.iter().enumerate() {
        let mut moved = values.to_vec();
        if let Some(v) = moved.get_mut(*dc) {
            *v += 1.0;
        }
        for (r, row) in rows.iter().enumerate() {
            if let (Some(cell), Some(b)) = (m.get_mut(r).and_then(|line| line.get_mut(c)), base.get(r)) {
                *cell = at(&moved, *row) - b;
            }
        }
    }
    for (r, line) in m.iter_mut().enumerate() {
        if let (Some(last), Some(w), Some(b)) = (line.get_mut(n), want.get(r), base.get(r)) {
            *last = w - b;
        }
    }
    let Some(delta) = solve_linear(m) else { return values.to_vec() };
    let mut out = values.to_vec();
    for (dc, d) in rows.iter().zip(delta) {
        if let Some(v) = out.get_mut(*dc) {
            *v += d;
        }
    }
    let fin = applied(degs, own, &out);
    let raw = own.ext;
    if out.iter().all(|v| v.is_finite()) && (fin.sized(&cx.co) || !raw.sized(&cx.co)) { out } else { values.to_vec() }
}

/// Solves a small system given as its augmented matrix, by elimination with the largest pivot. `None` for a system that is singular or has
/// numbers that are not finite.
fn solve_linear(mut m: Vec<Vec<f64>>) -> Option<Vec<f64>> {
    let n = m.len();
    for col in 0..n {
        let mag = |r: usize| m.get(r).and_then(|line| line.get(col)).map_or(0.0, |v| v.abs());
        let pivot = (col..n).fold(col, |best, r| if mag(r) > mag(best) { r } else { best });
        if m.get(pivot)?.get(col)?.abs() < 1.0e-9 {
            return None;
        }
        m.swap(col, pivot);
        let row = m.get(col)?.clone();
        for r in 0..n {
            if r == col {
                continue;
            }
            let k = m.get(r)?.get(col)? / row.get(col)?;
            for (c, v) in m.get_mut(r)?.iter_mut().enumerate() {
                *v -= k * row.get(c)?;
            }
        }
    }
    let out: Vec<f64> = (0..n).map(|i| Some(m.get(i)?.get(n)? / m.get(i)?.get(i)?)).collect::<Option<Vec<f64>>>()?;
    out.iter().all(|v| v.is_finite()).then_some(out)
}

/// The extents after every degree took its value.
fn applied(degs: &[Degree], own: &Own, values: &[f64]) -> Ext {
    degs.iter().zip(values).fold(own.ext, |e, (d, g)| d.kind.apply(e, *g))
}

/// Resizes a box: its moving sides along its own axes. Each degree takes its best candidate; a candidate that does not hold on the final
/// extents (a corner two moving sides share) is dropped and the degree's next is tried, so that what is drawn is where the box lands.
pub(crate) fn solve_edges(cx: &mut Ctx<'_>, _detail: &mut dyn Detail, frame: TurnedBox, moving: Edges) -> Solution {
    cx.region = cx.t.rel_bounds(&frame);
    let own = Own::of(&frame, cx.o);
    let dirs = directions(cx, &frame);
    let degs = degrees(cx, &own, &moving);
    let keep = if matches!(cx.q.rounding, Rounding::None) { 1 } else { 8 };
    let mut lists: Vec<Vec<Cand>> = Vec::new();
    for deg in &degs {
        let cands = degree_cands(cx, &own, deg, &dirs);
        lists.push(top(cands, keep.max(4), cx.s.choose, &cx.co));
    }
    let cap = u64::from(cx.budget().solves);
    loop {
        let values = round_values(cx, &own, &degs, &lists.iter().map(|l| l.first().map_or(0.0, |c| c.d)).collect::<Vec<f64>>());
        let fin = applied(&degs, &own, &values);
        let collapsed = own.ext.sized(&cx.co) && !fin.sized(&cx.co);
        let bad = if collapsed {
            lists.iter().rposition(|l| !l.is_empty())
        } else {
            lists.iter().position(|l| l.first().is_some_and(|c| !holds(cx, &own, c, &fin)))
        };
        let Some(i) = bad else { break };
        if !crate::budget::Work::charge(&mut cx.work.solves, cap, 1, &mut cx.work.cut, &mut cx.work.truncated, crate::budget::Cuts::SOLVES) {
            lists.iter_mut().for_each(Vec::clear);
            break;
        }
        if let Some(l) = lists.get_mut(i) {
            l.remove(0);
        }
    }
    let values = round_values(cx, &own, &degs, &lists.iter().map(|l| l.first().map_or(0.0, |c| c.d)).collect::<Vec<f64>>());
    let mut sol = Solution::new();
    for c in lists.iter().filter_map(|l| l.first()) {
        sol.hits.push(if c.kind == Kind::Size { HitRec::Size(*c) } else { HitRec::Line(*c) });
    }
    let out = own.rebuild(applied(&degs, &own, &values), cx.o, &frame);
    sol.k = out.centre - frame.centre;
    sol.frame = Some(out);
    sol
}

/// The probes of the final box of a resize: the points of each moving side in each direction.
pub(crate) fn edge_probes(cx: &Ctx<'_>, frame: &TurnedBox, moving: &Edges) -> Vec<Probe> {
    let own = Own::of(frame, cx.o);
    let mut out = Vec::new();
    for (angle, family) in directions(cx, frame) {
        let (a, flipped) = crate::index::canon(angle, cx.co.angle);
        let n = crate::frame::unit(a);
        let t = Vec2::new(-n.y, n.x);
        let mut sides = Vec::new();
        for id in carrying(cx, moving) {
            for p in own.side_points(id, &own.ext) {
                sides.push(ProbeSide {
                    side: crate::frame::canon_side(side_label(id), flipped),
                    own: family,
                    offset: n.dot(p),
                    lo: t.dot(p),
                    hi: t.dot(p),
                });
            }
        }
        if !sides.is_empty() {
            out.push(Probe { angle: a, normal: n, sides });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Coincide;

    #[test]
    fn a_box_is_sized_when_each_extent_is_more_than_the_same_place_rule_can_tell_from_nothing() {
        let co = Coincide::default();
        let ext = |w: f64, h: f64| Ext { a0: 0.0, a1: w, b0: 0.0, b1: h };
        assert!(ext(5.0, 5.0).sized(&co));
        assert!(!ext(co.abs, 5.0).sized(&co), "a width of exactly the rule is none");
        assert!(!ext(5.0, co.abs).sized(&co), "a height of exactly the rule is none");
        assert!(ext(2.0 * co.abs, 2.0 * co.abs).sized(&co));
        assert!(!ext(-1.0, 5.0).sized(&co) && !ext(5.0, 0.0).sized(&co));
    }

    #[test]
    fn a_degree_is_valid_when_the_box_it_leaves_is_sized_and_its_value_is_a_number() {
        let co = Coincide::default();
        let e = Ext { a0: 0.0, a1: 4.0, b0: 0.0, b1: 10.0 };
        let side = DegKind::Side(SideId::UMax);
        assert!(side.valid(&co, e, 1.0) && side.valid(&co, e, -3.0));
        assert!(!side.valid(&co, e, -4.0), "no width left");
        assert!(!side.valid(&co, e, -5.0), "flipped");
        assert!(!side.valid(&co, e, f64::INFINITY) && !side.valid(&co, e, f64::NAN));
        assert!(!DegKind::Side(SideId::VMin).valid(&co, e, 10.0));
        assert!(DegKind::Half(true).valid(&co, e, -1.0) && !DegKind::Half(true).valid(&co, e, -2.0));
    }
}
