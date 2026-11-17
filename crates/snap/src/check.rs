//! The check of a result: that every mark is true on the geometry the result returns, and that the constraint and the rounding hold there.
//!
//! It reads the targets by brute force (every object, every line entry, every lattice), not through the indexes, so that an index that
//! misses a line cannot also hide a mark that is not true.

use std::f64::consts::PI;

use kurbo::{Point, Vec2};

use crate::frame::{Axis, Coincide, unit};
use crate::index::canon;
use crate::kind::Kind;
use crate::query::{About, Constraint, Query, Rounding, Subject};
use crate::result::{Broken, Mark, Snapped};
use crate::targets::{SideLine, Targets, side_kind, side_lines};

fn broken(rule: &'static str, what: impl std::fmt::Debug) -> Result<(), Broken> {
    Err(Broken { rule, what: format!("{what:?}") })
}

/// Whether two directions, as normal angles, are one within the tolerance (the half turn wraps).
fn same_direction(a: f64, b: f64, co: &Coincide) -> bool {
    let x = (a - b).rem_euclid(PI);
    x.min(PI - x) <= co.angle
}

/// The box the app gave for the subject's page directions, at the subject's final place.
fn final_bounds(q: &Query<'_>, f: &crate::frame::TurnedBox) -> Option<kurbo::Rect> {
    match q.subject {
        Subject::Box { bounds: Some(b), frame, .. } | Subject::Nudge { bounds: Some(b), frame, .. } => Some(b + (f.centre - frame.centre)),
        _ => None,
    }
}

/// The sides of the final geometry, in the canonical form of a line: the canonical angle, the offset and the extent.
fn final_sides(t: &Targets, snapped: &Snapped, q: &Query<'_>) -> Vec<SideLine> {
    let co = &q.settings.coincide;
    if let Some(f) = snapped.frame {
        return side_lines(&f, final_bounds(q, &f), t.origin, co, true);
    }
    match snapped.point {
        Some(p) => {
            let rel = p - t.origin;
            let mut angles = vec![0.0, std::f64::consts::FRAC_PI_2];
            angles.extend(t.app_directions());
            angles
                .into_iter()
                .map(|a| {
                    let (n, tn) = (unit(a), Vec2::new(-unit(a).y, unit(a).x));
                    SideLine {
                        angle: a,
                        side: crate::frame::Side::Mid,
                        own: crate::frame::OwnOrBounds::Bounds,
                        offset: n.dot(rel),
                        lo: tn.dot(rel),
                        hi: tn.dot(rel),
                    }
                })
                .collect()
        }
        None => Vec::new(),
    }
}

/// Whether the targets hold a line of `kind` in the direction `angle` (canonical) at `c`.
fn target_line(t: &Targets, q: &Query<'_>, kind: Kind, angle: f64, c: f64) -> bool {
    let co = &q.settings.coincide;
    let in_entries = t
        .app
        .dirs
        .iter()
        .filter(|d| same_direction(d.angle, angle, co))
        .flat_map(|d| d.sets.iter())
        .filter(|s| s.kind == kind)
        .any(|s| s.entries.iter().any(|e| co.same(e.offset, c)));
    if in_entries {
        return true;
    }
    let from_object = t.objects.iter().any(|o| {
        (o.parts.bounds_lines || o.parts.own_lines)
            && side_lines(&o.frame, o.bounds, t.origin, co, o.parts.own_lines).iter().any(|l| {
                let (a, flipped) = canon(l.angle, co.angle);
                side_kind(l.side) == kind && same_direction(a, angle, co) && co.same(if flipped { -l.offset } else { l.offset }, c)
            })
    });
    from_object || lattice_line(t, q, kind, angle, c)
}

fn lattice_line(t: &Targets, q: &Query<'_>, kind: Kind, angle: f64, c: f64) -> bool {
    let co = &q.settings.coincide;
    q.lattices.iter().filter(|l| l.kind == kind).any(|l| {
        let (origin, pitch, o) = if same_direction(angle, 0.0, co) {
            (l.origin.x, l.pitch.x, t.origin.x)
        } else if same_direction(angle, std::f64::consts::FRAC_PI_2, co) {
            (l.origin.y, l.pitch.y, t.origin.y)
        } else {
            return false;
        };
        let n = ((o + c - origin) / pitch).round();
        pitch.is_finite() && pitch > 0.0 && co.same(origin + n * pitch - o, c)
    })
}

/// A target line on a mark's line: its kind, the side of its box it is (`None` for a line with no side) and its extent along the line (empty, `lo > hi`,
/// for a line that runs on for ever).
#[derive(Clone, Copy, Debug)]
struct OnLine {
    kind: Kind,
    side: Option<crate::frame::Side>,
    lo: f64,
    hi: f64,
}

/// Every line of the targets, by brute force, in the direction `ac` at the offset `c`: the app's, each object's and the lattices'. A line the
/// builder grouped with others at one place is each of its members here, with its own extent.
fn lines_on(t: &Targets, q: &Query<'_>, ac: f64, c: f64) -> Vec<OnLine> {
    let co = &q.settings.coincide;
    let mut out = Vec::new();
    for d in t.app.dirs.iter().filter(|d| same_direction(d.angle, ac, co)) {
        for set in &d.sets {
            for e in set.entries.iter().filter(|e| co.same(e.offset, c)) {
                let members = e.m0 as usize..(e.m0 + e.mn) as usize;
                match set.members.get(members).filter(|m| e.mn > 0 && !m.is_empty()) {
                    Some(m) => out.extend(m.iter().map(|x| OnLine { kind: set.kind, side: set.side, lo: x.lo, hi: x.hi })),
                    None => out.push(OnLine { kind: set.kind, side: set.side, lo: e.lo, hi: e.hi }),
                }
            }
        }
    }
    for o in &t.objects {
        for l in side_lines(&o.frame, o.bounds, t.origin, co, o.parts.own_lines) {
            let wanted = if l.own == crate::frame::OwnOrBounds::Own { o.parts.own_lines } else { o.parts.bounds_lines || o.parts.own_lines };
            let (a, flipped) = canon(l.angle, co.angle);
            let (off, lo, hi) = if flipped { (-l.offset, -l.hi, -l.lo) } else { (l.offset, l.lo, l.hi) };
            if wanted && same_direction(a, ac, co) && co.same(off, c) {
                out.push(OnLine { kind: side_kind(l.side), side: Some(crate::frame::canon_side(l.side, flipped)), lo, hi });
            }
        }
    }
    for l in q.lattices {
        if lattice_line(t, q, l.kind, ac, c) {
            out.push(OnLine { kind: l.kind, side: None, lo: f64::INFINITY, hi: f64::NEG_INFINITY });
        }
    }
    out
}

/// Whether the matching rule lets a side of the subject meet a line of `kind` that is `target` of its box (or none).
fn may_meet(matching: crate::settings::Matching, kind: Kind, side: crate::frame::Side, target: Option<crate::frame::Side>) -> bool {
    use crate::settings::Matching;
    match matching {
        Matching::AnyToAny => true,
        Matching::LikeToLike => !kind.has_side() || ((kind == Kind::Edge) == (side != crate::frame::Side::Mid)),
        Matching::SameSide => !kind.has_side() || target.is_none_or(|t| t == side),
    }
}

/// The rules a line mark keeps about which targets it stands for and how far it reaches along its line: every kind it names is a line a side of
/// the subject may meet under the matching rule, and the segment is the subject's extent on the line joined with every target's extent (under
/// `Every`) or with the extent of the one target nearest the subject and no other (under `Nearest`).
fn line_targets(t: &Targets, q: &Query<'_>, snapped: &Snapped, m: &Mark, ac: f64, c: f64, seg: (f64, f64)) -> Result<(), Broken> {
    let Mark::Line { kinds, .. } = m else { return Ok(()) };
    let (co, s) = (&q.settings.coincide, q.settings);
    // Far from the origin the same-place rule is widened once by the query and again by the check, so the check sees lines at one place that the query
    // kept apart: which targets a mark stands for is not asked there.
    if snapped.far(t, q) > 1.0e6 {
        return Ok(());
    }
    let mut subject: Vec<(crate::frame::Side, f64, f64)> = final_sides(t, snapped, q)
        .into_iter()
        .filter(|l| snapped.frame.is_none() || if l.own == crate::frame::OwnOrBounds::Own { s.frames.own } else { s.frames.page })
        .filter_map(|l| {
            let (a, flipped) = canon(l.angle, co.angle);
            let (off, lo, hi) = if flipped { (-l.offset, -l.hi, -l.lo) } else { (l.offset, l.lo, l.hi) };
            (same_direction(a, ac, co) && co.same(off, c)).then_some((crate::frame::canon_side(l.side, flipped), lo, hi))
        })
        .collect();
    let lines = lines_on(t, q, ac, c);
    if subject.is_empty() {
        return Ok(());
    }
    for k in kinds {
        let met = lines.iter().any(|l| l.kind == *k && subject.iter().any(|(side, _, _)| may_meet(s.matching, *k, *side, l.side)));
        if !met {
            return broken("a line mark names a target no side of the subject may meet under the matching rule", m);
        }
    }
    // Which sides and lines the mark stands for.
    let named: Vec<OnLine> = lines
        .into_iter()
        .filter(|l| kinds.contains(&l.kind) && subject.iter().any(|(side, _, _)| may_meet(s.matching, l.kind, *side, l.side)))
        .collect();
    subject.retain(|(side, _, _)| named.iter().any(|l| may_meet(s.matching, l.kind, *side, l.side)));
    // Under a reach only the objects near the subject offer their lines, which this brute force does not repeat: the extents are asked of a
    // query that has none.
    if s.align_reach.is_some_and(|r| !matches!(r, crate::kind::Reach::Any)) {
        return Ok(());
    }
    let exact = match q.subject {
        Subject::Point { .. } | Subject::Nudge { .. } => true,
        Subject::Box { u, v, .. } => u == crate::frame::Anchors::ALL && v == crate::frame::Anchors::ALL,
        _ => false,
    };
    let spans: Vec<(f64, f64)> = named.iter().filter(|l| l.lo <= l.hi).map(|l| (l.lo, l.hi)).collect();
    let ext = subject.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |e, (_, lo, hi)| (e.0.min(*lo), e.1.max(*hi)));
    let joined = |with: &[(f64, f64)]| with.iter().fold(ext, |e, s| (e.0.min(s.0), e.1.max(s.1)));
    let same_seg = |e: (f64, f64)| co.same(seg.0, e.0) && co.same(seg.1, e.1);
    let covers = |e: (f64, f64)| seg.0 <= e.0 + co.tol(seg.0, e.0) && seg.1 >= e.1 - co.tol(seg.1, e.1);
    match s.line_span {
        crate::settings::LineSpan::Every => {
            // The segment covers every target's extent (and, when the subject's sides are all known, is exactly their join).
            if !spans.iter().all(|sp| covers(*sp)) || (exact && !same_seg(joined(&spans))) {
                return broken("a line mark does not reach every target on its line", m);
            }
        }
        crate::settings::LineSpan::Nearest => {
            let gap = |sp: &(f64, f64)| {
                if sp.1 < ext.0 {
                    ext.0 - sp.1
                } else if sp.0 > ext.1 {
                    sp.0 - ext.1
                } else {
                    0.0
                }
            };
            let least = spans.iter().map(gap).fold(f64::INFINITY, f64::min);
            let tied: Vec<(f64, f64)> = spans.iter().copied().filter(|sp| co.same(gap(sp), least) || gap(sp) <= least).collect();
            let reaches = spans.is_empty() || tied.iter().any(|sp| covers(*sp));
            // No extent beyond the subject's and the nearest target's: one of the tied nearest, and no other.
            let exactly = !exact || spans.is_empty() || tied.iter().any(|sp| same_seg(joined(&[*sp])));
            if !(reaches && exactly) {
                return broken("a line mark does not reach the nearest target on its line, or reaches another", m);
            }
        }
    }
    Ok(())
}

/// A line mark: a side of the final geometry is on it, its segment covers that side, and a line of each kind it names is in the targets.
fn line_mark(t: &Targets, q: &Query<'_>, snapped: &Snapped, m: &Mark) -> Result<(), Broken> {
    let Mark::Line { direction, offset, segment, kinds, .. } = m else { return Ok(()) };
    let co = &q.settings.coincide;
    let (ac, _) = canon(direction.angle(), co.angle);
    let (n, tn) = (unit(ac), Vec2::new(-unit(ac).y, unit(ac).x));
    let c = n.dot(segment.p0 - t.origin);
    let c_off = c;
    let (t0, t1) = (tn.dot(segment.p0 - t.origin), tn.dot(segment.p1 - t.origin));
    if !(segment.p0.is_finite() && segment.p1.is_finite() && offset.is_finite()) {
        return broken("a line mark is not finite", m);
    }
    if !co.same(n.dot(segment.p1 - t.origin), c) {
        return broken("a line mark's segment is not along its line", m);
    }
    let on = final_sides(t, snapped, q).into_iter().any(|s| {
        let (a, flipped) = canon(s.angle, co.angle);
        let (off, lo, hi) = if flipped { (-s.offset, -s.hi, -s.lo) } else { (s.offset, s.lo, s.hi) };
        let tol = co.tol(t0.max(t1), lo.max(hi));
        same_direction(a, ac, co) && co.same(off, c) && t0.min(t1) <= lo + tol && t0.max(t1) >= hi - tol
    });
    // A resized box's moved sides meet lines in other directions through their ends and middles.
    let through_point = matches!(q.subject, Subject::Edges { .. })
        && snapped.frame.is_some_and(|f| {
            let c = f.corners();
            let mids = [c[0].midpoint(c[1]), c[1].midpoint(c[2]), c[2].midpoint(c[3]), c[3].midpoint(c[0])];
            c.into_iter().chain(mids).any(|p| {
                let tp = tn.dot(p - t.origin);
                co.same(n.dot(p - t.origin), c_off) && t0.min(t1) <= tp + co.tol(tp, t0) && t0.max(t1) >= tp - co.tol(tp, t1)
            })
        });
    if !(on || through_point) {
        return broken("no side of the final geometry is on a line mark, or its segment does not cover it", m);
    }
    for k in kinds {
        if !target_line(t, q, *k, ac, c) {
            return broken("a line mark names a target that is not in the set", m);
        }
    }
    // Which targets it stands for under the matching rule, and how far it reaches (a side in the page's directions only: the other marks of a
    // resize go through a point, not a side).
    // (A resize labels the sides it moves by its own axes, which a box turned by whole quarters has the other way round from its page sides, so the
    // labels are not compared there.)
    if on && !matches!(q.subject, Subject::Edges { .. }) {
        line_targets(t, q, snapped, m, ac, c, (t0.min(t1), t0.max(t1)))?;
    }
    Ok(())
}

/// The constraint holds on the returned geometry.
fn constraint(snapped: &Snapped, q: &Query<'_>) -> Result<(), Broken> {
    let co = &q.settings.coincide;
    let reference = snapped.frame.map(|f| f.centre).or(snapped.point);
    match q.constraint {
        Constraint::Axis(a) if matches!(q.subject, Subject::Point { .. } | Subject::Box { .. }) => {
            let off = if a == Axis::X { snapped.delta.y } else { snapped.delta.x };
            if off != 0.0 {
                return broken("an axis lock moved the other axis", snapped.delta);
            }
        }
        _ if matches!(q.subject, Subject::Nudge { .. }) => {
            let Subject::Nudge { axis, .. } = q.subject else { return Ok(()) };
            let off = if axis == Axis::X { snapped.delta.y } else { snapped.delta.x };
            if off != 0.0 {
                return broken("a nudge left its axis", snapped.delta);
            }
        }
        Constraint::Ray { origin, dir } => {
            if let Some(p) = reference {
                let e = dir / dir.hypot();
                let off = (p - origin).cross(e).abs();
                if off > co.tol(off, p.x.abs().max(p.y.abs())) {
                    return broken("the reference point left the ray", p);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn on_lattice(co: &Coincide, v: f64, step: f64) -> bool {
    !(step.is_finite() && step > 0.0) || co.same(v, (v / step).round() * step) || !(v / step).is_finite()
}

/// A point of a resized box that leads the drag, and the page axes it leads along (x, y).
type LeadingPoint = (Point, (bool, bool));

/// The points of a resized box whose coordinate the rounding sets, each with the page axes it may be set along (a point that leads along `x` is
/// on the lattice in `x`; where its two components are equal it may lead along either). For plain sides that is the middle of each moving side, led
/// along the page axis its own axis lies nearer to; for sides held together about the centre, the middle of the high side of each axis; under a
/// kept aspect it is the dragged corner (or the middle of the dragged side), the carried point farthest from the fixed corner or the centre.
/// The second part says whether one of them is enough: under a kept aspect there is one scale, so one leading coordinate, and the points
/// tied for farthest (a corner dragged about the centre) are alternatives.
fn leading_points(q: &Query<'_>, f: &crate::frame::TurnedBox, moving: &crate::query::Edges) -> (Vec<LeadingPoint>, bool) {
    let c = f.corners();
    let (u, v) = (f.u(), f.v());
    let mid = |a: Point, b: Point| a.midpoint(b);
    let axes = |w: Vec2| {
        let (x, y) = (w.x.abs(), w.y.abs());
        let tie = (x - y).abs() <= 1.0e-6 * x.max(y);
        (x >= y || tie, y >= x || tie)
    };
    // The side and its three points: min-u, max-u, min-v, max-v.
    let side = |i: usize| -> [Point; 3] {
        match i {
            0 => [c[0], mid(c[3], c[0]), c[3]],
            1 => [c[1], mid(c[1], c[2]), c[2]],
            2 => [c[0], mid(c[0], c[1]), c[1]],
            _ => [c[3], mid(c[3], c[2]), c[2]],
        }
    };
    let flags = [moving.u_min, moving.u_max, moving.v_min, moving.v_max];
    match q.constraint {
        Constraint::Centre => {
            let mut out = Vec::new();
            if flags[0] || flags[1] {
                out.push((side(1)[1], axes(u)));
            }
            if flags[2] || flags[3] {
                out.push((side(3)[1], axes(v)));
            }
            (out, false)
        }
        Constraint::Aspect { about, .. } => {
            // The point the scale is about: the fixed corner, side middle or centre of the final box (the high side moved when `u_max` did).
            let off = |moves_one_end: bool, high: bool, len: f64| {
                if about == About::Fixed && moves_one_end { if high { -0.5 * len } else { 0.5 * len } } else { 0.0 }
            };
            let (mu, mv) = (moving.u_min != moving.u_max, moving.v_min != moving.v_max);
            let fixed_point = f.centre + u * off(mu, moving.u_max, f.size.width) + v * off(mv, moving.v_max, f.size.height);
            let mut carried: Vec<(Point, Vec2)> = Vec::new();
            for (i, on) in flags.iter().enumerate() {
                if *on {
                    carried.extend(side(i).iter().map(|p| (*p, *p - fixed_point)));
                }
            }
            let far = carried.iter().map(|(_, w)| w.hypot()).fold(0.0, f64::max);
            let one_side = flags.iter().filter(|m| **m).count() == 1;
            let out = carried
                .into_iter()
                .filter(|(p, w)| {
                    w.hypot() > 0.0 && w.hypot() >= far * (1.0 - 1.0e-9) || (one_side && *p == side(flags.iter().position(|m| *m).unwrap_or(0))[1])
                })
                .map(|(p, w)| (p, axes(w)))
                .collect();
            (out, true)
        }
        _ => {
            let mut out = Vec::new();
            for (i, on) in flags.iter().enumerate() {
                if *on {
                    out.push((side(i)[1], axes(if i < 2 { u } else { v })));
                }
            }
            (out, false)
        }
    }
}

/// A resize under a rounding: the points that lead the drag are on the lattice along the page axis they lead on.
fn resize_rounding(snapped: &Snapped, q: &Query<'_>, moving: &crate::query::Edges) -> Result<(), Broken> {
    let (Some(f), co) = (snapped.frame, &q.settings.coincide) else { return Ok(()) };
    let (step, from) = match q.rounding {
        Rounding::None => return Ok(()),
        Rounding::Position(step) => (step, Point::ORIGIN),
        Rounding::Delta { step, from } => (step, from),
    };
    let valid = |s: f64| s.is_finite() && s > 0.0;
    // The leading points of two sides are at least 0.7 of the box's side apart along the axis they lead on; a step wider than that can round both to one
    // line, and then the drag stands as it was given: the check asks only where the rounding can be made.
    let small = step.x.max(step.y) < 0.7 * f.size.width.min(f.size.height);
    if !(valid(step.x) && valid(step.y)) || !small || f.size.width <= 0.0 || f.size.height <= 0.0 {
        return Ok(());
    }
    let (points, any) = leading_points(q, &f, moving);
    let on = |(p, (x, y)): &(Point, (bool, bool))| {
        // A coordinate it leads along: both when its two components are alike.
        let ok_x = on_lattice(co, p.x - from.x, step.x);
        let ok_y = on_lattice(co, p.y - from.y, step.y);
        (!*x || ok_x) && (!*y || ok_y) && (*x || *y)
    };
    let held = if any { points.iter().any(on) } else { points.iter().all(on) };
    if held { Ok(()) } else { broken("a resize's leading point is not on the lattice along the axis it leads on", (f, points.len())) }
}

/// The rounding holds: the distance moved, or the min corner, is on its lattice, on the axes the constraint leaves to it.
fn rounding(snapped: &Snapped, q: &Query<'_>) -> Result<(), Broken> {
    // A nudge goes to its target or by its step: the step is the app's, and a rounding is not asked of it.
    if matches!(q.subject, Subject::Nudge { .. }) {
        return Ok(());
    }
    if let Subject::Edges { moving, .. } = q.subject {
        return resize_rounding(snapped, q, &moving);
    }
    let co = &q.settings.coincide;
    let free = |a: Axis| match q.constraint {
        Constraint::Axis(l) => l == a,
        Constraint::Ray { dir, .. } => (a == Axis::X) == (dir.x.abs() >= dir.y.abs()),
        _ => true,
    };
    let (refp, minp) = match (snapped.frame, snapped.point) {
        (Some(f), _) => {
            let b = f.bounds();
            (f.centre, Point::new(b.x0, b.y0))
        }
        (None, Some(p)) => (p, p),
        _ => return Ok(()),
    };
    let axes = [(Axis::X, refp.x, minp.x), (Axis::Y, refp.y, minp.y)];
    for (axis, r, m) in axes.into_iter().filter(|(a, _, _)| free(*a)) {
        let ok = match q.rounding {
            Rounding::None => true,
            Rounding::Delta { step, from } => {
                let (s, f) = if axis == Axis::X { (step.x, from.x) } else { (step.y, from.y) };
                on_lattice(co, r - f, s)
            }
            Rounding::Position(step) => on_lattice(co, m, if axis == Axis::X { step.x } else { step.y }),
        };
        if !ok {
            return broken("the rounding does not hold", (axis, refp, minp));
        }
    }
    Ok(())
}

/// The geometry the result returns is the subject moved by the delta it returns, and a box keeps its size.
fn landed_where_told(snapped: &Snapped, q: &Query<'_>) -> Result<(), Broken> {
    let co = &q.settings.coincide;
    match (q.subject, snapped.frame, snapped.point) {
        (Subject::Box { frame, .. } | Subject::Nudge { frame, .. }, Some(f), _) => {
            if !(co.same_point(frame.centre + snapped.delta, f.centre) && frame.size == f.size) {
                return broken("the returned box is not the subject moved by the returned delta", (frame, f, snapped.delta));
            }
        }
        (Subject::Point { at, .. }, _, Some(p)) if !co.same_point(at + snapped.delta, p) => {
            return broken("the returned point is not the subject moved by the returned delta", (at, p, snapped.delta));
        }
        _ => {}
    }
    Ok(())
}

/// A turn under a hard lattice lands on it.
fn hard_lattice(snapped: &Snapped, q: &Query<'_>) -> Result<(), Broken> {
    let (Constraint::Angles { step, zero }, Some(a)) = (q.constraint, snapped.angle) else { return Ok(()) };
    if matches!(q.subject, Subject::Rotate { .. }) && !crate::angle::on_lattice(a, step, zero, crate::angle::tolerance(&q.settings.coincide, a, zero))
    {
        return broken("a turn under a hard lattice is not on it", a);
    }
    Ok(())
}

/// The angle of a moved subject is the subject's, to the bit.
fn angle_unchanged(snapped: &Snapped, q: &Query<'_>) -> Result<(), Broken> {
    let before = match q.subject {
        Subject::Box { frame, .. } | Subject::Edges { frame, .. } | Subject::Nudge { frame, .. } => Some(frame.angle),
        _ => None,
    };
    match (before, snapped.frame) {
        (Some(a), Some(f)) if a.to_bits() != f.angle.to_bits() => broken("the angle of the subject changed", (a, f.angle)),
        _ => Ok(()),
    }
}

/// Every hit has a mark that says so, unless the marks were cut.
fn hits_have_marks(snapped: &Snapped, q: &Query<'_>) -> Result<(), Broken> {
    let co = &q.settings.coincide;
    if snapped.work.cut.has(crate::budget::Cuts::MARKS) {
        return Ok(());
    }
    for h in &snapped.hits {
        let found = snapped.marks.iter().any(|m| match (h.kind, m) {
            (Kind::Spacing, Mark::Gap { direction, hit: true, .. }) => h.direction.is_none_or(|d| same_direction(direction.angle(), d.angle(), co)),
            (Kind::Size, Mark::Size { hit: true, .. }) | (Kind::Angle | Kind::AngleStep, Mark::Angle { hit: true, .. }) => true,
            (
                Kind::Point(_)
                | Kind::Corner
                | Kind::SideMiddle
                | Kind::Vertex
                | Kind::Midpoint
                | Kind::Nearest
                | Kind::Intersection
                | Kind::Tangent
                | Kind::Perpendicular,
                Mark::Place { hit: true, .. },
            ) => true,
            (k, Mark::Line { direction, kinds, hit: true, .. }) => {
                h.direction.is_some_and(|d| same_direction(direction.angle(), d.angle(), co)) && kinds.contains(&k)
            }
            _ => false,
        });
        if !found {
            return broken("a hit has no mark", h);
        }
    }
    Ok(())
}

/// A nudge stops on its target strictly ahead, within its reach, or moves exactly by the fallback when there is none.
fn nudge_stops(snapped: &Snapped, q: &Query<'_>) -> Result<(), Broken> {
    let Subject::Nudge { axis, forward, reach, fallback, .. } = q.subject else { return Ok(()) };
    let co = &q.settings.coincide;
    let e = axis.unit() * if forward { 1.0 } else { -1.0 };
    let d = snapped.delta.dot(e);
    if !d.is_finite() {
        return broken("a nudge did not move by a number", snapped.delta);
    }
    let stops: Vec<f64> = snapped.hits.iter().map(|h| h.distance).collect();
    if stops.is_empty() {
        // No target ahead: exactly the fallback.
        return if co.same(d, fallback) { Ok(()) } else { broken("a nudge with no target ahead did not move by the fallback", (d, fallback)) };
    }
    if d <= co.tol(d, 0.0) {
        return broken("a nudge did not stop strictly ahead of where it began", d);
    }
    if !stops.iter().any(|s| co.same(*s, d)) {
        return broken("a nudge did not stop where its target is", (d, stops));
    }
    if let crate::query::NudgeReach::Within(step) = reach
        && d > step + co.tol(d, step)
    {
        return broken("a nudge went farther than its reach", (d, step));
    }
    Ok(())
}

/// An angle mark: the angle is the result's, another object has it (modulo the period), and the lattice has it.
fn angle_mark(t: &Targets, q: &Query<'_>, snapped: &Snapped, m: &Mark) -> Result<(), Broken> {
    let Mark::Angle { angle, matched, step, .. } = m else { return Ok(()) };
    let (co, p) = (&q.settings.coincide, q.settings.angle_period);
    if snapped.angle.is_none_or(|a| a.to_bits() != angle.to_bits()) {
        return broken("an angle mark is not the final angle", m);
    }
    if let Some(id) = matched {
        let ok = t.objects.iter().any(|o| {
            o.id == *id
                && o.parts.angle
                && crate::angle::wrap(o.angle.unwrap_or(o.frame.angle) - angle, p).abs() <= crate::angle::tolerance(co, *angle, 0.0)
        });
        if !ok {
            return broken("an angle mark names an object that does not have the angle", m);
        }
    }
    if let Some(st) = step {
        let zero = match (q.subject, q.constraint) {
            (_, Constraint::Angles { zero, .. }) => zero,
            (Subject::Rotate { step: Some((_, z)), .. }, _) => z,
            _ => return broken("an angle mark has a step but the query has none", m),
        };
        if !crate::angle::on_lattice(*angle, *st, zero, crate::angle::tolerance(co, *angle, zero)) {
            return broken("an angle mark's angle is not a whole number of steps", m);
        }
    }
    Ok(())
}

/// A size mark: the length is the subject's final own width or height, its side is that side of the final box, and the matched object has a
/// side of that length.
fn size_mark(t: &Targets, q: &Query<'_>, snapped: &Snapped, m: &Mark) -> Result<(), Broken> {
    let Mark::Size { value, side, matched, count, .. } = m else { return Ok(()) };
    let (co, Some(f)) = (&q.settings.coincide, snapped.frame) else { return broken("a size mark without a final box", m) };
    if !(co.same(*value, f.size.width) || co.same(*value, f.size.height)) {
        return broken("a size mark is not the final width or height", m);
    }
    let corners = f.corners();
    let on_corner = |p: Point| corners.iter().any(|c| co.same_point(*c, p));
    if !(on_corner(side.p0) && on_corner(side.p1) && co.same((side.p1 - side.p0).hypot(), *value)) || *count == 0 {
        return broken("a size mark's side is not a side of the final box", m);
    }
    if let Some((id, line)) = matched {
        let in_view = |o: &crate::targets::ObjectEntry| match (q.settings.kinds.rule(Kind::Size).scope, q.view) {
            (crate::kind::Scope::View, Some(v)) => {
                let (b, view) = (o.bounds.unwrap_or_else(|| o.frame.bounds()), v.view());
                b.x0 <= view.x1 && view.x0 <= b.x1 && b.y0 <= view.y1 && view.y0 <= b.y1
            }
            _ => true,
        };
        let found = t
            .objects
            .iter()
            .any(|o| o.id == *id && o.parts.size && in_view(o) && (co.same(o.frame.size.width, *value) || co.same(o.frame.size.height, *value)));
        if !found || !co.same((line.p1 - line.p0).hypot(), *value) {
            return broken("a size mark names an object that has no side of that length", m);
        }
    }
    Ok(())
}

/// A resized box keeps the sides that do not move, within the tolerance, and keeps its centre when opposite sides move together.
fn resized(snapped: &Snapped, q: &Query<'_>, t: &Targets) -> Result<(), Broken> {
    let (Subject::Edges { frame, moving }, Some(f)) = (q.subject, snapped.frame) else { return Ok(()) };
    let co = &q.settings.coincide;
    let ext = |b: &crate::frame::TurnedBox| {
        let (u, v) = (b.u(), b.v());
        let c = (b.centre - t.origin).to_point().to_vec2();
        let (cu, cv) = (u.dot(c), v.dot(c));
        [cu - 0.5 * b.size.width, cu + 0.5 * b.size.width, cv - 0.5 * b.size.height, cv + 0.5 * b.size.height]
    };
    let (before, after) = (ext(&frame), ext(&f));
    let tiny = |len: f64| len <= co.tol(len, 0.0);
    if !tiny(frame.size.width) && !tiny(frame.size.height) && (tiny(f.size.width) || tiny(f.size.height)) {
        return broken("a resize collapsed or flipped the box", f);
    }
    let flags = [moving.u_min, moving.u_max, moving.v_min, moving.v_max];
    match q.constraint {
        Constraint::Centre => {
            if !co.same_point(frame.centre, f.centre) {
                return broken("a resize about the centre moved the centre", f);
            }
        }
        Constraint::Aspect { ratio, .. } => {
            if !co.same(f.size.width, ratio * f.size.height) {
                return broken("a resize that keeps the proportions changed them", f);
            }
        }
        _ => {
            for i in 0..4 {
                if !flags.get(i).copied().unwrap_or(false) && !co.same(before.get(i).copied().unwrap_or(0.0), after.get(i).copied().unwrap_or(0.0)) {
                    return broken("a side that does not move moved", (i, f));
                }
            }
        }
    }
    Ok(())
}

/// A place mark: a place of the final geometry is on it, and a place of the kinds the set holds itself is in the set.
fn place_mark(t: &Targets, q: &Query<'_>, snapped: &Snapped, m: &Mark) -> Result<(), Broken> {
    let Mark::Place { at, kind, id, number, .. } = m else { return Ok(()) };
    let co = &q.settings.coincide;
    let finals: Vec<Point> = match (q.subject, snapped.point, snapped.frame) {
        (Subject::Point { .. }, Some(p), _) => vec![p],
        (Subject::Box { own, .. }, _, Some(f)) => crate::places::box_places(&f).into_iter().chain(own.iter().map(|p| *p + snapped.delta)).collect(),
        _ => Vec::new(),
    };
    if !finals.iter().any(|p| co.same_point(*p, *at)) {
        return broken("a place mark is not on a place of the final geometry", m);
    }
    let exists = match kind {
        Kind::Point(_) => t.points.iter().any(|p| p.kind == *kind && p.id == *id && Some(p.number) == *number && co.same_point(p.at, *at)),
        Kind::Corner | Kind::SideMiddle => t.objects.iter().any(|o| {
            let c = o.frame.corners();
            let places: Vec<Point> = if *kind == Kind::Corner {
                c.to_vec()
            } else {
                vec![c[0].midpoint(c[1]), c[1].midpoint(c[2]), c[2].midpoint(c[3]), c[3].midpoint(c[0])]
            };
            Some(o.id) == *id && places.iter().any(|p| co.same_point(*p, *at))
        }),
        _ => true,
    };
    if !exists {
        return broken("a place mark names a place that is not in the set", m);
    }
    Ok(())
}

/// A gap mark: its segment runs along its direction, its length is its value, and its ends are on facing sides of objects of the set (or of the
/// subject), which overlap across the segment. Gaps of one chain have one value.
fn gap_mark(t: &Targets, q: &Query<'_>, snapped: &Snapped, m: &Mark) -> Result<(), Broken> {
    let Mark::Gap { direction, segment, value, chain, .. } = m else { return Ok(()) };
    let co = &q.settings.coincide;
    let (n, tn) = (direction.normal(), direction.tangent());
    let (a, b) = (n.dot(segment.p0 - t.origin), n.dot(segment.p1 - t.origin));
    let across = tn.dot(segment.p0 - t.origin);
    let (lo, hi) = (a.min(b), a.max(b));
    if !(co.same(tn.dot(segment.p1 - t.origin), across) && co.same(hi - lo, *value) && *value > 0.0) {
        return broken("a gap mark is not along its direction, or its length is not its value", m);
    }
    // A row along a page axis reads the box round an outline where the app gave one (the subject's moves with it); a row along a turned box's own
    // axis reads the frames. Rows of each kind are the ones whose direction is a page axis or is not.
    let page = {
        let a = direction.angle();
        a <= co.angle || (a - std::f64::consts::FRAC_PI_2).abs() <= co.angle || (a - std::f64::consts::PI).abs() <= co.angle
    };
    let reads = |f: &crate::frame::TurnedBox, bounds: Option<kurbo::Rect>| match bounds.filter(|_| page) {
        Some(r) => crate::spacing::outline_extents(&r, n, t.origin),
        None => (crate::spacing::extent(f, n, t.origin), crate::spacing::extent(f, tn, t.origin)),
    };
    let subject_box = snapped.frame.map(|f| {
        let bounds = match q.subject {
            Subject::Box { bounds: Some(b), frame: from, .. } | Subject::Nudge { bounds: Some(b), frame: from, .. } => {
                Some(b + (f.centre - from.centre))
            }
            _ => None,
        };
        (f, bounds)
    });
    let boxes = t.objects.iter().filter(|o| o.parts.spacing).map(|o| (o.frame, o.bounds)).chain(subject_box);
    let (mut left, mut right) = (false, false);
    for (f, bounds) in boxes {
        let ((l, h), (c0, c1)) = reads(&f, bounds);
        let covers = c0 <= across + co.tol(c0, across) && across <= c1 + co.tol(c1, across);
        left |= covers && co.same(h, lo);
        right |= covers && co.same(l, hi);
    }
    if !(left && right) {
        return broken("an end of a gap mark is not on a facing side of an object", m);
    }
    let chain_values: Vec<f64> =
        snapped.marks.iter().filter_map(|x| if let Mark::Gap { chain: c, value: v, .. } = x { (c == chain).then_some(*v) } else { None }).collect();
    if chain_values.iter().any(|v| !co.same(*v, *value)) {
        return broken("a chain holds gaps of different values", m);
    }
    Ok(())
}

impl Snapped {
    /// Checks both ways that every mark is true. It must hold on the final geometry, the constraint and the rounding, and the target it names
    /// must be in the set: a line of those kinds there, the row's objects at a gap's ends, an object of that length or angle, a place there.
    /// This is what the tests assert.
    pub fn check(&self, targets: &Targets, query: &Query<'_>) -> Result<(), Broken> {
        // Far from the origin the geometry the result carries is only as exact as the numbers there: the same-place rule is widened by what a
        // number of that size cannot say, so a result is not called false for the rounding of its own coordinates.
        let far = self.far(targets, query);
        let mut settings = query.settings.clone();
        settings.coincide = settings.coincide.at(far);
        let widened = Query { settings: &settings, ..query.clone() };
        self.check_with(targets, &widened)
    }

    /// The largest coordinate of the subject, the final geometry and the targets' origin.
    fn far(&self, targets: &Targets, query: &Query<'_>) -> f64 {
        let big = |p: Point| if p.x.is_finite() && p.y.is_finite() { p.x.abs().max(p.y.abs()) } else { 0.0 };
        let frame = |f: &crate::frame::TurnedBox| f.corners().iter().map(|c| big(*c)).fold(0.0, f64::max);
        query.subject.far().max(self.frame.as_ref().map_or(0.0, frame)).max(self.point.map_or(0.0, big)).max(big(targets.origin))
    }

    fn check_with(&self, targets: &Targets, query: &Query<'_>) -> Result<(), Broken> {
        if self.work.refused.is_some() {
            return if self.delta == Vec2::ZERO && self.marks.is_empty() { Ok(()) } else { broken("a refused query moved the subject", self.delta) };
        }
        if self.angle.is_some_and(|a| !a.is_finite()) {
            return broken("the angle is not a finite number", self.angle);
        }
        landed_where_told(self, query)?;
        angle_unchanged(self, query)?;
        hard_lattice(self, query)?;
        constraint(self, query)?;
        rounding(self, query)?;
        resized(self, query, targets)?;
        nudge_stops(self, query)?;
        for m in &self.marks {
            line_mark(targets, query, self, m)?;
            size_mark(targets, query, self, m)?;
            angle_mark(targets, query, self, m)?;
            gap_mark(targets, query, self, m)?;
            place_mark(targets, query, self, m)?;
        }
        if self.work.marks_dropped != 0 {
            return broken("marks were dropped", self.work.marks_dropped);
        }
        hits_have_marks(self, query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_directions_are_one_when_they_differ_by_a_half_turn_or_by_less_than_the_tolerance() {
        let co = Coincide::default();
        assert!(same_direction(0.0, 0.0, &co));
        assert!(same_direction(0.3, 0.3 + PI, &co), "a half turn apart is the same line");
        assert!(same_direction(0.0, PI - 1.0e-12, &co) && same_direction(PI - 1.0e-12, 0.0, &co), "just under a half turn is just over 0");
        assert!(!same_direction(0.3, 0.5, &co));
        assert!(!same_direction(0.3, PI - 0.3, &co), "mirrored about the half turn is not the same");
        assert!(!same_direction(0.0, PI / 2.0, &co));
    }
}
