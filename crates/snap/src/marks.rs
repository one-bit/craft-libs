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
use crate::settings::{LineSpan, MarkRule, SizeAxes};
use crate::solve::{HitRec, Solution};

/// The lines one probe's sides lie on at one place.
struct Group {
    c: f64,
    kinds: Vec<Kind>,
    first: Option<Id>,
    count: u32,
    /// The extents of the lines found, and of the subject's sides on this line.
    spans: Vec<(f64, f64)>,
    ext: (f64, f64),
    lo: f64,
    hi: f64,
    hit: bool,
}

/// The probes of the final geometry of the subject.
fn final_probes(cx: &Ctx<'_>, sol: &Solution) -> Vec<Probe> {
    match (cx.q.subject, sol.point, sol.frame) {
        (Subject::Point { .. }, Some(p), _) => point_probes(cx, p),
        (Subject::Box { u, v, .. }, _, Some(f)) => box_probes(cx, &f, u, v),
        (Subject::Nudge { .. }, _, Some(f)) => box_probes(cx, &f, crate::frame::Anchors::ALL, crate::frame::Anchors::ALL),
        (Subject::Edges { moving, .. }, _, Some(f)) => crate::resize::edge_probes(cx, &f, &moving),
        _ => Vec::new(),
    }
}

fn is_hit(sol: &Solution, probe: &Probe, c: f64, cx: &Ctx<'_>) -> bool {
    sol.hits.iter().any(|h| match h {
        HitRec::Line(cand) => (cand.angle - probe.angle).abs() <= cx.co.angle && cx.co.same(cand.c, c),
        HitRec::Size(_) | HitRec::Angle(_) | HitRec::Place(_) => false,
    })
}

/// The groups of lines one probe's sides lie on.
fn groups_of(cx: &mut Ctx<'_>, sol: &Solution, probe: &Probe) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for ps in &probe.sides {
        for f in lines_find(cx, probe.angle, ps.offset, ps.side, Some((ps.lo, ps.hi))) {
            let at = match groups.iter().position(|g| cx.co.same(g.c, f.c)) {
                Some(i) => i,
                None => {
                    let hit = is_hit(sol, probe, f.c, cx);
                    groups.push(Group {
                        c: f.c,
                        kinds: Vec::new(),
                        first: None,
                        count: 0,
                        spans: Vec::new(),
                        ext: (f64::INFINITY, f64::NEG_INFINITY),
                        lo: f64::INFINITY,
                        hi: f64::NEG_INFINITY,
                        hit,
                    });
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
            g.ext = (g.ext.0.min(ps.lo), g.ext.1.max(ps.hi));
            if f.lo <= f.hi {
                g.spans.push((f.lo, f.hi));
            }
        }
    }
    groups.sort_by(|a, b| a.c.total_cmp(&b.c));
    let nearest = cx.s.line_span == LineSpan::Nearest;
    for g in &mut groups {
        g.kinds.sort();
        // Every target on the line, or the one nearest the subject and no other.
        let gap = |s: &(f64, f64)| {
            if s.1 < g.ext.0 {
                g.ext.0 - s.1
            } else if s.0 > g.ext.1 {
                s.0 - g.ext.1
            } else {
                0.0
            }
        };
        let chosen: Vec<(f64, f64)> = if nearest {
            g.spans.iter().copied().min_by(|a, b| gap(a).total_cmp(&gap(b)).then(a.0.total_cmp(&b.0))).into_iter().collect()
        } else {
            g.spans.clone()
        };
        (g.lo, g.hi) = chosen.iter().fold(g.ext, |e, s| (e.0.min(s.0), e.1.max(s.1)));
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
    let (widths, heights) = cx.lengths();
    let arrays: Vec<&[crate::index::Length]> = match (cx.s.size_axes, width) {
        (SizeAxes::EitherAxis, _) => vec![widths, heights],
        (SizeAxes::SameAxis, true) => vec![widths],
        (SizeAxes::SameAxis, false) => vec![heights],
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
    (count, hit.filter(|_| named.is_some() && count > 0).or(named))
}

/// The size marks of a resized box: for each own axis that a moving side changes, the final length against other objects' equal lengths.
fn size_marks(cx: &mut Ctx<'_>, sol: &Solution, frame: &TurnedBox, moving: &crate::query::Edges) -> Vec<Mark> {
    let mut out = Vec::new();
    let on_axis = |width: bool| sol.hits.iter().any(|h| matches!(h, HitRec::Size(c) if (c.angle == 0.0) == width));
    for (width, on) in [(true, moving.u_min || moving.u_max || on_axis(true)), (false, moving.v_min || moving.v_max || on_axis(false))] {
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
    let a = sol.angle.filter(|a| a.is_finite())?;
    let p = cx.s.angle_period;
    let t = cx.t;
    let arr = t.angles(p, &mut cx.work.made).unwrap_or(&[]);
    cx.work.bisect_steps = cx.work.bisect_steps.saturating_add(crate::index::log_steps(arr.len()));
    let a_f = a.rem_euclid(p);
    let tol = crate::angle::tolerance(&cx.co, a, 0.0);
    let same = |l: &crate::index::Length| crate::angle::wrap(l.length - a_f, p).abs() <= tol;
    let i = arr.partition_point(|l| l.length < a_f);
    let named = [i.checked_sub(1), Some(i), Some(0), arr.len().checked_sub(1)]
        .into_iter()
        .flatten()
        .filter_map(|j| arr.get(j))
        .filter(|l| same(l))
        .map(|l| l.id)
        .min();
    let hit = sol.hits.iter().find_map(|h| match h {
        HitRec::Angle(x) => Some(*x),
        _ => None,
    });
    let hard = match cx.q.constraint {
        crate::query::Constraint::Angles { step, zero } => Some((step, zero)),
        _ => None,
    };
    let lattice = hard.or(step).filter(|(st, z)| {
        crate::angle::on_lattice(a, *st, *z, crate::angle::tolerance(&cx.co, a, *z)) && (hard.is_some() || cx.rule(Kind::AngleStep).on)
    });
    let matched = named.filter(|_| cx.rule(Kind::Angle).on);
    if matched.is_none() && lattice.is_none() {
        return None;
    }
    let hit_id = hit.and_then(|h| h.id);
    Some(Mark::Angle { pivot, angle: a, matched: hit_id.filter(|_| matched.is_some()).or(matched), step: lattice.map(|l| l.0), hit: hit.is_some() })
}

/// The places of the final geometry of the subject: a point, or the nine places of a box and the places that move with it.
pub(crate) fn final_places(cx: &Ctx<'_>, sol: &Solution) -> Vec<kurbo::Point> {
    match (cx.q.subject, sol.point, sol.frame) {
        (Subject::Point { .. }, Some(p), _) => vec![p],
        (Subject::Box { own, .. }, _, Some(f)) => crate::places::box_places(&f).into_iter().chain(own.iter().map(|p| *p + sol.k)).collect(),
        _ => Vec::new(),
    }
}

/// A place mark for each target place that was weighed and that a place of the final geometry is on.
fn place_marks(cx: &Ctx<'_>, sol: &Solution) -> Vec<Mark> {
    let finals = final_places(cx, sol);
    let mut out: Vec<Mark> = Vec::new();
    for g in &cx.gathered {
        if !finals.iter().any(|q| cx.co.same_point(*q, g.at)) {
            continue;
        }
        let hit = sol.hits.iter().any(
            |h| matches!(h, HitRec::Place(p) if p.g.kind == g.kind && p.g.id == g.id && p.g.number == g.number && cx.co.same_point(p.g.at, g.at)),
        );
        let dup = out.iter().any(|m| matches!(m, Mark::Place { at, kind, id, number, .. } if *kind == g.kind && *id == g.id && *number == g.number && cx.co.same_point(*at, g.at)));
        if !dup && (cx.s.marks != MarkRule::HitsOnly || hit) {
            out.push(Mark::Place { at: g.at, kind: g.kind, id: g.id, number: g.number, hit });
        }
    }
    out
}

/// The marks of the final geometry.
pub(crate) fn derive(cx: &mut Ctx<'_>, sol: &Solution) -> Vec<Mark> {
    let mut out = Vec::new();
    if let Subject::Rotate { pivot, step, .. } = cx.q.subject {
        let hit_only = cx.s.marks == MarkRule::HitsOnly;
        let mark = angle_mark(cx, sol, pivot, step).filter(|m| !hit_only || matches!(m, Mark::Angle { hit: true, .. }));
        // The angle mark pays the marks budget like every other mark.
        let (cap, mut spent) = (u64::from(cx.budget().marks), 0u64);
        if let Some(m) = mark.filter(|_| Work::charge(&mut spent, cap, 1, &mut cx.work.cut, &mut cx.work.truncated, Cuts::MARKS)) {
            out.push(m);
        }
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
        if let (Subject::Box { .. } | Subject::Nudge { .. }, Some(f)) = (cx.q.subject, sol.frame) {
            // The chains of one direction are numbered on from those of the directions before it: one number is one chain.
            let before = out.iter().filter_map(|m| if let Mark::Gap { chain, .. } = m { Some(*chain) } else { None }).max().unwrap_or(0);
            let gaps = crate::spacing::gap_marks(cx, sol, &f, &probe);
            let gaps = gaps
                .into_iter()
                .filter(|m| cx.s.marks != MarkRule::HitsOnly || matches!(m, Mark::Gap { hit: true, .. }))
                .map(|m| match m {
                    Mark::Gap { chain, direction, segment, value, subject, hit } => {
                        Mark::Gap { chain: chain.saturating_add(before), direction, segment, value, subject, hit }
                    }
                    other => other,
                })
                .collect();
            let n = out.len();
            out.extend(crate::spacing::within_cap(cx, gaps, n));
        }
    }
    if let (Subject::Edges { moving, .. }, Some(f)) = (cx.q.subject, sol.frame) {
        out.extend(size_marks(cx, sol, &f, &moving));
    }
    if matches!(cx.q.subject, Subject::Point { .. } | Subject::Box { .. }) {
        let places = place_marks(cx, sol);
        let n = out.len();
        out.extend(crate::spacing::within_cap(cx, places, n));
    }
    out
}
