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

    /// Whether the extents it gives are a box with some size on both axes.
    fn valid(&self, e: Ext, g: f64) -> bool {
        let n = self.apply(e, g);
        g.is_finite() && n.a1 - n.a0 > 0.0 && n.b1 - n.b0 > 0.0
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

/// What the settings and the query make of the constraint of a resize: a corner drag keeps its proportions when the settings say so.
pub(crate) fn effective_constraint(s: &crate::settings::Settings, c: Constraint, moving: &Edges, frame: &TurnedBox) -> Constraint {
    let one = |a: bool, b: bool| a != b;
    let corner = one(moving.u_min, moving.u_max) && one(moving.v_min, moving.v_max);
    match c {
        Constraint::None if s.corners_keep_proportions && corner && frame.size.height > 0.0 => {
            Constraint::Aspect { ratio: frame.size.width / frame.size.height, about: About::Fixed }
        }
        other => other,
    }
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
fn degrees(cx: &Ctx<'_>, own: &Own, moving: &Edges, frame: &TurnedBox) -> Vec<Degree> {
    let sides = flagged(moving);
    let e = own.ext;
    let carry = |ids: &[SideId], w: &dyn Fn(SideId, Vec2) -> Vec2| -> Vec<(Vec2, Vec2, SideId, u8)> {
        ids.iter()
            .flat_map(|id| own.side_points(*id, &e).into_iter().enumerate().map(move |(i, p)| (p, *id, i as u8)))
            .map(|(p, id, i)| (p, w(id, p), id, i))
            .collect()
    };
    match effective_constraint(cx.s, cx.q.constraint, moving, frame) {
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
            let (a, _) = crate::index::canon(*angle, cx.co.angle);
            let n = crate::frame::unit(a);
            let nw = n.dot(*w);
            if nw.abs() < 1.0e-12 {
                continue;
            }
            let s = n.dot(*p0);
            for f in lines_near(cx, a, s, side_label(*id)) {
                let g = (f.c - s) / nw;
                let travel = g.abs() * w.hypot();
                if !g.is_finite() || !cx.pulls(f.kind, travel) || !deg.kind.valid(own.ext, g) {
                    continue;
                }
                out.push(Cand {
                    kind: f.kind,
                    id: f.id,
                    prio: cx.rule(f.kind).priority,
                    reach: cx.reach(f.kind),
                    side: side_label(*id),
                    own: *family,
                    angle: a,
                    c: f.c,
                    s,
                    d: g,
                    t: travel,
                    rate: nw,
                    pt: side_code(*id) * 4 + idx,
                    count: f.count,
                });
            }
        }
    }
    out.extend(size_cands(cx, own, deg));
    out
}

/// The length in a sorted array nearest to `len` whose object counts under the scope of the settings. With a view, the walk goes out from the
/// insertion point past the objects that are outside it, charged to `line_steps` and cut there.
pub(crate) fn nearest_length(cx: &mut Ctx<'_>, arr: &[crate::index::Length], len: f64) -> Option<crate::index::Length> {
    let i = arr.partition_point(|l| l.length < len);
    let (below, above) = arr.split_at(i.min(arr.len()));
    let cap = u64::from(cx.budget().line_steps);
    let mut found: Vec<crate::index::Length> = Vec::new();
    for side in [0, 1] {
        let list: Box<dyn Iterator<Item = &crate::index::Length> + '_> =
            if side == 0 { Box::new(below.iter().rev()) } else { Box::new(above.iter()) };
        for l in list {
            if !crate::budget::Work::charge(
                &mut cx.work.line_steps,
                cap,
                1,
                &mut cx.work.cut,
                &mut cx.work.truncated,
                crate::budget::Cuts::LINE_STEPS,
            ) {
                break;
            }
            if cx.in_scope(l.id) {
                found.push(*l);
                break;
            }
        }
    }
    found.into_iter().min_by(|a, b| (a.length - len).abs().total_cmp(&(b.length - len).abs()).then(a.id.cmp(&b.id)))
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
    let (widths, heights) = (cx.t.widths(&mut cx.work.made), cx.t.heights(&mut cx.work.made));
    let mut out = Vec::new();
    for (is_w, len, rate, per_g) in changes {
        let arrays: Vec<&[crate::index::Length]> = match (cx.s.size_axes, is_w) {
            (SizeAxes::EitherAxis, _) => vec![widths, heights],
            (SizeAxes::SameAxis, true) => vec![widths],
            (SizeAxes::SameAxis, false) => vec![heights],
        };
        for arr in arrays {
            cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(crate::index::log_steps(arr.len()));
            let Some(m) = nearest_length(cx, arr, len) else { continue };
            let g = (m.length - len) / rate;
            let travel = g.abs() * per_g;
            if !(g.is_finite() && cx.pulls(Kind::Size, travel) && deg.kind.valid(own.ext, g)) {
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
                rate,
                pt: 0,
                count: 1,
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

/// The rounding of the sides of an unturned box that move: each lands on the lattice of the step (`Position` only).
fn rounded(cx: &Ctx<'_>, frame: &TurnedBox, own: &Own, deg: &DegKind, g: f64) -> f64 {
    let Rounding::Position(step) = cx.q.rounding else { return g };
    let (Some(q), DegKind::Side(id)) = (frame.quarter(cx.co.angle), deg) else { return g };
    if q != 0 {
        return g;
    }
    let (step_i, base) = if matches!(id, SideId::UMin | SideId::UMax) {
        (step.x, if *id == SideId::UMin { own.ext.a0 } else { own.ext.a1 })
    } else {
        (step.y, if *id == SideId::VMin { own.ext.b0 } else { own.ext.b1 })
    };
    let abs = base + if matches!(id, SideId::UMin | SideId::UMax) { cx.o.x } else { cx.o.y };
    if !(step_i.is_finite() && step_i > 0.0) {
        return g;
    }
    let want = ((abs + g) / step_i).round() * step_i;
    if want.is_finite() { want - abs } else { g }
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
    let degs = degrees(cx, &own, &moving, &frame);
    let keep = if matches!(cx.q.rounding, Rounding::None) { 1 } else { 8 };
    let mut lists: Vec<Vec<Cand>> = Vec::new();
    for deg in &degs {
        let cands = degree_cands(cx, &own, deg, &dirs);
        lists.push(top(cands, keep.max(4), cx.s.choose, &cx.co));
    }
    let cap = u64::from(cx.budget().solves);
    loop {
        let values: Vec<f64> = degs.iter().zip(&lists).map(|(d, l)| rounded(cx, &frame, &own, &d.kind, l.first().map_or(0.0, |c| c.d))).collect();
        let fin = applied(&degs, &own, &values);
        let collapsed = own.ext.a1 > own.ext.a0 && own.ext.b1 > own.ext.b0 && !(fin.a1 > fin.a0 && fin.b1 > fin.b0);
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
    let values: Vec<f64> = degs.iter().zip(&lists).map(|(d, l)| rounded(cx, &frame, &own, &d.kind, l.first().map_or(0.0, |c| c.d))).collect();
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
        let (a, _) = crate::index::canon(angle, cx.co.angle);
        let n = crate::frame::unit(a);
        let t = Vec2::new(-n.y, n.x);
        let mut sides = Vec::new();
        for id in carrying(cx, moving) {
            for p in own.side_points(id, &own.ext) {
                sides.push(ProbeSide { side: side_label(id), own: family, offset: n.dot(p), lo: t.dot(p), hi: t.dot(p) });
            }
        }
        if !sides.is_empty() {
            out.push(Probe { angle: a, normal: n, sides });
        }
    }
    out
}
