//! What to draw, derived from the final geometry. The winning hits are not carried forward: after the geometry is known, each side of it is
//! looked up again among the lines, so a mark is there if and only if the geometry lies on its line.

use kurbo::{Line, Vec2};

use crate::align::lines_find;
use crate::budget::{Cuts, Work};
use crate::ctx::{Ctx, Probe};
use crate::frame::{Direction, Id, TurnedBox};
use crate::kind::Kind;
use crate::moves::{box_probes, point_probes};
use crate::query::Subject;
use crate::result::Mark;
use crate::settings::{MarkRule, SizeAxes};
use crate::solve::{HitRec, Solution};

/// The lines one probe's sides lie on at one place.
struct Group {
    c: f64,
    kinds: Vec<Kind>,
    first: Option<Id>,
    count: u32,
    lo: f64,
    hi: f64,
    hit: bool,
}

/// The probes of the final geometry of the subject.
fn final_probes(cx: &Ctx<'_>, sol: &Solution) -> Vec<Probe> {
    match (cx.q.subject, sol.point, sol.frame) {
        (Subject::Point { .. }, Some(p), _) => point_probes(cx, p),
        (Subject::Box { u, v, .. }, _, Some(f)) => box_probes(cx, &f, u, v),
        (Subject::Edges { moving, .. }, _, Some(f)) => crate::resize::edge_probes(cx, &f, &moving),
        _ => Vec::new(),
    }
}

fn is_hit(sol: &Solution, probe: &Probe, c: f64, cx: &Ctx<'_>) -> bool {
    sol.hits.iter().any(|h| match h {
        HitRec::Line(cand) => (cand.angle - probe.angle).abs() <= cx.co.angle && cx.co.same(cand.c, c),
        HitRec::Size(_) | HitRec::Angle(_) => false,
    })
}

/// The groups of lines one probe's sides lie on.
fn groups_of(cx: &mut Ctx<'_>, sol: &Solution, probe: &Probe) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for ps in &probe.sides {
        for f in lines_find(cx, probe.angle, ps.offset, ps.side, true) {
            let at = match groups.iter().position(|g| cx.co.same(g.c, f.c)) {
                Some(i) => i,
                None => {
                    let hit = is_hit(sol, probe, f.c, cx);
                    groups.push(Group { c: f.c, kinds: Vec::new(), first: None, count: 0, lo: f64::INFINITY, hi: f64::NEG_INFINITY, hit });
                    groups.len() - 1
                }
            };
            let Some(g) = groups.get_mut(at) else { continue };
            if !g.kinds.contains(&f.kind) {
                g.kinds.push(f.kind);
            }
            g.count = g.count.saturating_add(f.count);
            g.first = match (g.first, f.id) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            g.lo = g.lo.min(f.lo).min(ps.lo);
            g.hi = g.hi.max(f.hi).max(ps.hi);
        }
    }
    groups.sort_by(|a, b| a.c.total_cmp(&b.c));
    for g in &mut groups {
        g.kinds.sort();
    }
    groups
}

fn mark_of(cx: &Ctx<'_>, probe: &Probe, g: &Group) -> Option<Mark> {
    let n = probe.normal;
    let t = Vec2::new(-n.y, n.x);
    let at = |s: f64| cx.o + n * g.c + t * s;
    let (a, b) = (at(g.lo), at(g.hi));
    let direction = Direction::new(probe.angle)?;
    let offset = direction.normal().dot(a.to_vec2());
    (a.is_finite() && b.is_finite() && offset.is_finite()).then(|| Mark::Line {
        direction,
        offset,
        segment: Line::new(a, b),
        kinds: g.kinds.clone(),
        first: g.first,
        count: g.count,
        hit: g.hit,
    })
}

/// The own side of a box along an axis: from its first corner to the next along `u` (the width) or along `v` (the height).
pub(crate) fn own_side(frame: &TurnedBox, width: bool) -> Line {
    let c = frame.corners();
    Line::new(c[0], if width { c[1] } else { c[3] })
}

/// The widths or heights of other objects that equal `value`: how many, and the one named (the hit's, else the lowest id).
fn equal_lengths(cx: &mut Ctx<'_>, value: f64, width: bool, hit: Option<Id>) -> (u32, Option<Id>) {
    let t = cx.t;
    let arrays: Vec<&[crate::index::Length]> = match (cx.s.size_axes, width) {
        (SizeAxes::EitherAxis, _) => vec![t.widths(&mut cx.work.made), t.heights(&mut cx.work.made)],
        (SizeAxes::SameAxis, true) => vec![t.widths(&mut cx.work.made)],
        (SizeAxes::SameAxis, false) => vec![t.heights(&mut cx.work.made)],
    };
    let cap = u64::from(cx.budget().line_steps);
    let (mut count, mut named) = (0u32, None::<Id>);
    for arr in arrays {
        cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(crate::index::log_steps(arr.len()));
        let i = arr.partition_point(|l| l.length < value);
        let (below, above) = arr.split_at(i.min(arr.len()));
        for l in below.iter().rev().take_while(|l| cx.co.same(l.length, value)).chain(above.iter().take_while(|l| cx.co.same(l.length, value))) {
            if !Work::charge(&mut cx.work.line_steps, cap, 1, &mut cx.work.cut, &mut cx.work.truncated, Cuts::LINE_STEPS) {
                break;
            }
            count = count.saturating_add(1);
            named = Some(named.map_or(l.id, |n| n.min(l.id)));
        }
    }
    (count, hit.filter(|h| named.is_some() && count > 0).or(named))
}

/// The size marks of a resized box: for each own axis that a moving side changes, the final length against other objects' equal lengths.
fn size_marks(cx: &mut Ctx<'_>, sol: &Solution, frame: &TurnedBox, moving: &crate::query::Edges) -> Vec<Mark> {
    let mut out = Vec::new();
    for (width, on) in [(true, moving.u_min || moving.u_max), (false, moving.v_min || moving.v_max)] {
        if !on {
            continue;
        }
        let value = if width { frame.size.width } else { frame.size.height };
        let hit = sol.hits.iter().find_map(|h| match h {
            HitRec::Size(c) if cx.co.same(c.c, value) => Some(c.id),
            _ => None,
        });
        let (count, named) = equal_lengths(cx, value, width, hit.flatten());
        if count == 0 || (cx.s.marks == MarkRule::HitsOnly && hit.is_none()) {
            continue;
        }
        let matched = named.and_then(|id| {
            let t = cx.t;
            let i = t.objects.partition_point(|o| o.id < id);
            t.objects.get(i).filter(|o| o.id == id).map(|o| {
                let by_width = cx.co.same(o.frame.size.width, value);
                (id, own_side(&o.frame, by_width))
            })
        });
        out.push(Mark::Size { value, side: own_side(frame, width), matched, count, hit: hit.is_some() });
    }
    out
}

/// The angle mark of a turn: the final angle against other objects' angles and against the lattice of steps.
fn angle_mark(cx: &mut Ctx<'_>, sol: &Solution, pivot: kurbo::Point, step: Option<(f64, f64)>) -> Option<Mark> {
    let a = sol.angle?;
    let p = cx.s.angle_period;
    let t = cx.t;
    let arr = t.angles(p, &mut cx.work.made).unwrap_or(&[]);
    cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(crate::index::log_steps(arr.len()));
    let a_f = a.rem_euclid(p);
    let same = |l: &crate::index::Length| crate::angle::wrap(l.length - a_f, p).abs() <= cx.co.angle;
    let i = arr.partition_point(|l| l.length < a_f);
    let named = [i.checked_sub(1), Some(i), Some(0), arr.len().checked_sub(1)].into_iter().flatten().filter_map(|j| arr.get(j)).filter(|l| same(l)).map(|l| l.id).min();
    let hit = sol.hits.iter().find_map(|h| match h {
        HitRec::Angle(x) => Some(*x),
        _ => None,
    });
    let hard = match cx.q.constraint {
        crate::query::Constraint::Angles { step, zero } => Some((step, zero)),
        _ => None,
    };
    let lattice = hard.or(step).filter(|(st, z)| crate::angle::on_lattice(a, *st, *z, cx.co.angle) && (hard.is_some() || cx.rule(Kind::AngleStep).on));
    let matched = named.filter(|_| cx.rule(Kind::Angle).on);
    if matched.is_none() && lattice.is_none() {
        return None;
    }
    let hit_id = hit.and_then(|h| h.id);
    Some(Mark::Angle { pivot, angle: a, matched: hit_id.filter(|_| matched.is_some()).or(matched), step: lattice.map(|l| l.0), hit: hit.is_some() })
}

/// The marks of the final geometry.
pub(crate) fn derive(cx: &mut Ctx<'_>, sol: &Solution) -> Vec<Mark> {
    let mut out = Vec::new();
    if let Subject::Rotate { pivot, step, .. } = cx.q.subject {
        let hit_only = cx.s.marks == MarkRule::HitsOnly;
        out.extend(angle_mark(cx, sol, pivot, step).filter(|m| !hit_only || matches!(m, Mark::Angle { hit: true, .. })));
        return out;
    }
    let cap = u64::from(cx.budget().marks);
    for probe in final_probes(cx, sol) {
        for g in groups_of(cx, sol, &probe) {
            if cx.s.marks == MarkRule::HitsOnly && !g.hit {
                continue;
            }
            let Some(m) = mark_of(cx, &probe, &g) else { continue };
            let mut n = out.len() as u64;
            if !Work::charge(&mut n, cap, 1, &mut cx.work.cut, &mut cx.work.truncated, Cuts::MARKS) {
                return out;
            }
            out.push(m);
        }
    }
    if let (Subject::Edges { moving, .. }, Some(f)) = (cx.q.subject, sol.frame) {
        out.extend(size_marks(cx, sol, &f, &moving));
    }
    out
}
