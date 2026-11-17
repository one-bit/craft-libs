//! The check of a result: that every mark is true on the geometry the result returns, and that the constraint and the rounding hold there.
//!
//! It reads the targets by brute force (every object, every line entry, every lattice), not through the indexes, so that an index that
//! misses a line cannot also hide a mark that is not true.

use std::f64::consts::PI;

use kurbo::{Point, Vec2};

use crate::frame::{Axis, Coincide, unit};
use crate::index::canon;
use crate::kind::Kind;
use crate::query::{Constraint, Query, Rounding, Subject};
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

/// The rounding holds: the distance moved, or the min corner, is on its lattice, on the axes the constraint leaves to it.
fn rounding(snapped: &Snapped, q: &Query<'_>) -> Result<(), Broken> {
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
    if frame.size.width > 0.0 && frame.size.height > 0.0 && (f.size.width <= 0.0 || f.size.height <= 0.0) {
        return broken("a resize collapsed or flipped the box", f);
    }
    let flags = [moving.u_min, moving.u_max, moving.v_min, moving.v_max];
    match q.constraint {
        Constraint::Centre => {
            if !co.same_point(frame.centre, f.centre) {
                return broken("a resize about the centre moved the centre", f);
            }
        }
        Constraint::Aspect { .. } => {
            if !co.same(f.size.width * frame.size.height, frame.size.width * f.size.height) {
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
    let boxes = t.objects.iter().filter(|o| o.parts.spacing).map(|o| o.frame).chain(snapped.frame);
    let (mut left, mut right) = (false, false);
    for f in boxes {
        let ((l, h), (c0, c1)) = (crate::spacing::extent(&f, n, t.origin), crate::spacing::extent(&f, tn, t.origin));
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
