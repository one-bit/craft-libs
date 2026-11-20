// Priority only on ties, and the bias of a place against a pair of lines, follow EffectCraft's `better` and `POINT_BIAS`: storytold/effectcraft at
// 60e2cbbbe3d73c0a9074c25a1a615ccfec18721c, crates/engine/src/viewer.rs. Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors. MIT OR Apache-2.0.
// Changed: up to four directions are paired, constraints and roundings are solved inside the choice, and a candidate is checked on the geometry it lands on.

//! A move: a point, or a box whose sides snap in up to four directions (the page's and its own), with a constraint and a rounding solved
//! inside the snap.
//!
//! Each direction gives at most one best candidate (a few more under a rounding, which may take a candidate off its line). Two candidates
//! in directions that are not parallel fix the correction exactly; one candidate alone moves the subject along its line's normal; a
//! constraint leaves one free degree, along which each candidate is solved. The combination that snaps the most degrees wins, then the
//! settings choose. A candidate that the rounding takes off its line is dropped and the next is tried, so what is drawn is where the subject
//! lands.

use std::cmp::Ordering;

use kurbo::{Point, Vec2};

use crate::align::{Free, probe_cands};
use crate::budget::{Cuts, Work};
use crate::ctx::{Cand, Ctx, Probe, ProbeSide, probes, top};
use crate::frame::{Anchors, OwnOrBounds, Side, TurnedBox, to_page_axes};
use crate::kind::Kind;
use crate::places::{PlaceCand, Travel, Way, box_places, solve_places};
use crate::query::{Constraint, Detail, Rounding, Subject};
use crate::settings::{Choose, Places, TravelPlaces};
use crate::solve::{HitRec, RoundOn, Solution, round_k};
use crate::targets::side_lines;

/// How many candidates a direction keeps: more when a rounding may take the best off its line.
fn keep(r: &Rounding) -> usize {
    if matches!(r, Rounding::None) { 1 } else { 8 }
}

/// How many more spacing candidates a direction keeps once a spacing candidate is among its best. A spacing candidate is read again on the geometry it
/// lands on, where the row may be another and the gap may no longer repeat; the direction would then have nothing left to try, though another of its
/// gaps holds there. They stand in for the kept one as single-degree combinations only: a pair is made of the candidates each direction kept.
const SPACING_FALLBACKS: usize = 7;

/// What a constraint leaves free: the way the candidates are solved, the base move, and which axes the rounding may change.
struct Freedom {
    free: Free,
    /// What is done to the raw drag before any target is looked at (the projection onto a ray).
    base: Vec2,
    on: RoundOn,
}

fn freedom(c: &Constraint, refp: Point) -> Freedom {
    match *c {
        Constraint::Axis(a) => Freedom { free: Free::Along(a.unit()), base: Vec2::ZERO, on: RoundOn::Only(a) },
        Constraint::Ray { origin, dir } => {
            let e = dir / dir.hypot();
            let p = origin + e * (refp - origin).dot(e);
            Freedom { free: Free::Along(e), base: p - refp, on: RoundOn::Lead(e) }
        }
        _ => Freedom { free: Free::Normal, base: Vec2::ZERO, on: RoundOn::Both },
    }
}

/// One way to move the subject: the correction and the candidates it puts on their lines.
#[derive(Clone, Debug)]
struct Combo {
    degrees: u8,
    k: Vec2,
    a: Cand,
    b: Option<Cand>,
}

impl Combo {
    fn worst_prio(&self) -> u8 {
        self.b.map_or(self.a.prio, |b| self.a.prio.max(b.prio))
    }

    fn best_prio(&self) -> u8 {
        self.b.map_or(self.a.prio, |b| self.a.prio.min(b.prio))
    }

    fn len(&self) -> f64 {
        self.k.hypot()
    }
}

/// Two candidates in directions that are not parallel fix the correction: `n_a · k = d_a` and `n_b · k = d_b`.
fn pair_k(a: &Cand, b: &Cand, na: Vec2, nb: Vec2) -> Option<Vec2> {
    let det = na.x * nb.y - na.y * nb.x;
    if det.abs() < 1.0e-12 {
        return None;
    }
    let k = Vec2::new((a.d * nb.y - b.d * na.y) / det, (na.x * b.d - nb.x * a.d) / det);
    k.is_finite().then_some(k)
}

fn combos(lists: &[Vec<Cand>], spares: &[Vec<Cand>], probes: &[Probe], free: Free) -> Vec<Combo> {
    let mut out = Vec::new();
    for (i, (list, spare)) in lists.iter().zip(spares).enumerate() {
        let Some(pi) = probes.get(i) else { continue };
        for a in list.iter().chain(spare) {
            let k = match free {
                Free::Normal => pi.normal * a.d,
                Free::Along(e) => e * a.d,
            };
            out.push(Combo { degrees: 1, k, a: *a, b: None });
        }
    }
    if !matches!(free, Free::Normal) {
        return out;
    }
    for (i, la) in lists.iter().enumerate() {
        for (j, lb) in lists.iter().enumerate().skip(i + 1) {
            let (Some(pa), Some(pb)) = (probes.get(i), probes.get(j)) else { continue };
            for a in la {
                for b in lb {
                    let Some(k) = pair_k(a, b, pa.normal, pb.normal) else { continue };
                    // Within the larger reach; and where both kinds have none, within a few times the farther of the two pulls (a pair of directions nearly
                    // parallel throws the box without end).
                    let reach = a.reach.max(b.reach);
                    let limit = if reach.is_finite() { reach } else { crate::ctx::MAX_AMPLIFICATION * a.t.max(b.t) };
                    if k.hypot() <= limit {
                        out.push(Combo { degrees: 2, k, a: *a, b: Some(*b) });
                    }
                }
            }
        }
    }
    out
}

/// Whether combination `x` is a better choice than `y`.
fn combo_order(x: &Combo, y: &Combo, choose: Choose, co: &crate::frame::Coincide) -> Ordering {
    let by_len = if co.same(x.len(), y.len()) { Ordering::Equal } else { x.len().total_cmp(&y.len()) };
    let prio = x.worst_prio().cmp(&y.worst_prio()).then(x.best_prio().cmp(&y.best_prio()));
    let rule = match choose {
        Choose::Nearest => by_len.then(prio),
        Choose::Ranked => prio.then(by_len),
    };
    let ids = |c: &Combo| (c.a.id, c.a.side, c.a.kind, c.b.and_then(|b| b.id));
    y.degrees.cmp(&x.degrees).then(rule).then(ids(x).cmp(&ids(y))).then(x.len().total_cmp(&y.len()))
}

fn best_combo(list: &[Combo], choose: Choose, co: &crate::frame::Coincide) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, c) in list.iter().enumerate() {
        if best.and_then(|b| list.get(b)).is_none_or(|b| combo_order(c, b, choose, co) == Ordering::Less) {
            best = Some(i);
        }
    }
    best
}

/// Whether the side that a candidate pulled is still on its line after the subject moved by `k`.
pub(crate) fn holds(cx: &Ctx<'_>, c: &Cand, k: Vec2) -> bool {
    let n = crate::frame::unit(c.angle);
    cx.co.same(c.s + n.dot(k), c.c)
}

/// A spacing candidate is checked again on the final geometry, where the row may not be the one it was found in.
fn spacing_holds(cx: &mut Ctx<'_>, c: &Cand, spaced: Option<(&TurnedBox, &[Probe])>, k: Vec2) -> bool {
    let (Kind::Spacing, Some((frame, probes))) = (c.kind, spaced) else { return true };
    let Some(probe) = probes.iter().find(|p| (p.angle - c.angle).abs() <= cx.co.angle) else { return false };
    crate::spacing::still_holds(cx, &TurnedBox { centre: frame.centre + k, ..*frame }, probe)
}

/// Chooses the combination, applies the rounding, and checks that every candidate it names still holds; if not, the next.
fn choose(
    cx: &mut Ctx<'_>,
    list: Vec<Combo>,
    fr: &Freedom,
    spaced: Option<(&TurnedBox, &[Probe])>,
    refp: Point,
    minp: Point,
) -> Option<(Vec2, Combo)> {
    let mut list = list;
    let cap = u64::from(cx.budget().solves);
    loop {
        let i = best_combo(&list, cx.s.choose, &cx.co)?;
        if !Work::charge(&mut cx.work.solves, cap, 1, &mut cx.work.cut, &mut cx.work.truncated, Cuts::SOLVES) {
            return None;
        }
        let combo = list.swap_remove(i);
        let k = fr.base + combo.k;
        let k = round_k(&cx.q.rounding, refp, minp, k, fr.on);
        let ok = [Some(combo.a), combo.b].into_iter().flatten().all(|c| holds(cx, &c, k - fr.base) && spacing_holds(cx, &c, spaced, k - fr.base));
        if ok {
            return Some((k, combo));
        }
    }
}

/// The box the app gave for the subject's page directions, moved with the subject.
pub(crate) fn subject_bounds(cx: &Ctx<'_>, frame: &TurnedBox) -> Option<kurbo::Rect> {
    match cx.q.subject {
        Subject::Box { bounds: Some(b), frame: f, .. } | Subject::Nudge { bounds: Some(b), frame: f, .. } => Some(b + (frame.centre - f.centre)),
        _ => None,
    }
}

/// The probes of a box: the sides of the box round it in the page's directions, and its own sides when it is turned.
pub(crate) fn box_probes(cx: &Ctx<'_>, frame: &TurnedBox, u: Anchors, v: Anchors) -> Vec<Probe> {
    let co = &cx.co;
    let (sq, xf, yf, turned) = match to_page_axes(frame, u, v, co.angle) {
        Some((b, x, y)) => (b, x, y, false),
        None => (*frame, u, v, true),
    };
    let (page, own) = (cx.s.frames.page, cx.s.frames.own && turned);
    let lines = side_lines(&sq, subject_bounds(cx, frame), cx.o, co, own);
    probes(&lines, co, &|l| match l.own {
        OwnOrBounds::Bounds => page && if l.angle == 0.0 { xf.has(l.side) } else { yf.has(l.side) },
        OwnOrBounds::Own => own && if l.angle == sq.angle { u.has(l.side) } else { v.has(l.side) },
    })
}

/// The probes of a point: the page's directions and those of the app's own lines.
pub(crate) fn point_probes(cx: &Ctx<'_>, at: Point) -> Vec<Probe> {
    let p = at - cx.o;
    let mut angles = vec![0.0, std::f64::consts::FRAC_PI_2];
    angles.extend(cx.t.app_directions());
    angles
        .into_iter()
        .filter(|a| cx.s.frames.page || (*a != 0.0 && *a != std::f64::consts::FRAC_PI_2))
        .map(|a| {
            let n = crate::frame::unit(a);
            let t = Vec2::new(-n.y, n.x);
            let side = ProbeSide { side: Side::Mid, own: OwnOrBounds::Bounds, offset: n.dot(p), lo: t.dot(p), hi: t.dot(p) };
            Probe { angle: a, normal: n, sides: vec![side] }
        })
        .collect()
}

/// The shared work of a move: candidates per probe, combinations, the choice.
fn solve_probes(cx: &mut Ctx<'_>, ps: &[Probe], fr: &Freedom, spaced: Option<&TurnedBox>, refp: Point, minp: Point) -> Option<(Vec2, Combo)> {
    let n = keep(&cx.q.rounding);
    let mut lists = Vec::new();
    let mut spares = Vec::new();
    for p in ps {
        let mut cands = probe_cands(cx, p, fr.free);
        let spacing = match spaced {
            Some(f) => crate::spacing::cands(cx, f, p, fr.free),
            None => Vec::new(),
        };
        cands.extend(spacing.iter().copied());
        let kept = top(cands, n, cx.s.choose, &cx.co);
        let spare = if kept.iter().any(|c| c.kind == Kind::Spacing) {
            let rest: Vec<Cand> = spacing.into_iter().filter(|s| !kept.contains(s)).collect();
            top(rest, SPACING_FALLBACKS, cx.s.choose, &cx.co)
        } else {
            Vec::new()
        };
        lists.push(kept);
        spares.push(spare);
    }
    let list = combos(&lists, &spares, ps, fr.free);
    choose(cx, list, fr, spaced.map(|f| (f, ps)), refp, minp)
}

/// What a move settled on: lines, or a place.
enum Chosen {
    Lines(Vec2, Combo),
    Place(Vec2, PlaceCand),
}

/// Lines and a place compete under the settings. A place is taken only if the rounding leaves the subject on it.
fn decide(cx: &Ctx<'_>, lines: Option<(Vec2, Combo)>, place: Option<PlaceCand>, fr: &Freedom, refp: Point, minp: Point) -> Option<Chosen> {
    let place = place.and_then(|p| {
        let k = round_k(&cx.q.rounding, refp, minp, p.k(), fr.on);
        cx.co.same_point(p.q + k, p.g.at).then_some((k, p))
    });
    match (lines, place) {
        (l, None) => l.map(|(k, c)| Chosen::Lines(k, c)),
        (None, Some((k, p))) => Some(Chosen::Place(k, p)),
        (Some((kl, c)), Some((kp, p))) => {
            let lines_win = match cx.s.places {
                Places::First => false,
                Places::Compete { bias } => {
                    c.degrees == 2
                        && match cx.s.choose {
                            Choose::Nearest => c.len() + bias * p.reach < p.dist,
                            Choose::Ranked => c.worst_prio() < p.prio,
                        }
                }
            };
            Some(if lines_win { Chosen::Lines(kl, c) } else { Chosen::Place(kp, p) })
        }
    }
}

/// Settles a move: its lines, its places, and the rounding of what it falls back to.
/// The subject's reference place and the min corner of the box round it, before it moves.
#[derive(Clone, Copy)]
struct Anchor {
    refp: Point,
    minp: Point,
}

fn settle(
    cx: &mut Ctx<'_>,
    detail: &mut dyn Detail,
    ps: &[Probe],
    fr: &Freedom,
    spaced: Option<&TurnedBox>,
    places: (Vec<Point>, Option<Point>),
    at: Anchor,
) -> (Vec2, Vec<HitRec>) {
    let (refp, minp) = (at.refp, at.minp);
    let lines = solve_probes(cx, ps, fr, spaced, refp, minp);
    let place = match (fr.free, cx.q.constraint) {
        (Free::Normal, _) => solve_places(cx, detail, &places.0, places.1, None),
        // A move held to a page axis meets the places on the lines its own places travel along.
        (Free::Along(_), Constraint::Axis(axis)) if cx.s.locked_places == TravelPlaces::OnLine => {
            solve_places(cx, detail, &places.0, places.1, Some(Travel { axis, way: Way::Both }))
        }
        _ => None,
    };
    match decide(cx, lines, place, fr, refp, minp) {
        Some(Chosen::Lines(k, c)) => (k, std::iter::once(c.a).chain(c.b).map(HitRec::Line).collect()),
        Some(Chosen::Place(k, p)) => (k, vec![HitRec::Place(p)]),
        None => (round_k(&cx.q.rounding, refp, minp, fr.base, fr.on), Vec::new()),
    }
}

/// Moves a point.
pub(crate) fn solve_point(cx: &mut Ctx<'_>, detail: &mut dyn Detail, at: Point, base: Option<Point>) -> Solution {
    let rel = (at - cx.o).to_point();
    cx.region = kurbo::Rect::from_points(rel, rel);
    let fr = freedom(&cx.q.constraint, at);
    let ps = point_probes(cx, at + fr.base);
    let (k, hits) = settle(cx, detail, &ps, &fr, None, (vec![at], base), Anchor { refp: at, minp: at });
    let mut sol = Solution::new();
    sol.k = k;
    sol.hits = hits;
    sol.point = Some(at + k);
    sol
}

/// The places of a box that are pulled: its nine, and the places of the objects that move with it (at most `own`).
fn subject_places(cx: &mut Ctx<'_>, frame: &TurnedBox, own: &[Point]) -> Vec<Point> {
    let mut out = box_places(frame);
    let cap = u64::from(cx.budget().own);
    for p in own.iter().filter(|p| p.is_finite()) {
        if !Work::charge(&mut cx.work.own, cap, 1, &mut cx.work.cut, &mut cx.work.truncated, Cuts::OWN) {
            break;
        }
        out.push(*p);
    }
    out
}

/// Moves a box.
pub(crate) fn solve_box(cx: &mut Ctx<'_>, detail: &mut dyn Detail, frame: TurnedBox, u: Anchors, v: Anchors, own: &[Point]) -> Solution {
    let fr = freedom(&cx.q.constraint, frame.centre);
    let moved = TurnedBox { centre: frame.centre + fr.base, ..frame };
    cx.region = cx.t.rel_bounds(&moved);
    let ps = box_probes(cx, &moved, u, v);
    let minp = Point::new(frame.bounds().x0, frame.bounds().y0);
    let places = subject_places(cx, &moved, own);
    let (k, hits) = settle(cx, detail, &ps, &fr, Some(&moved), (places, None), Anchor { refp: frame.centre, minp });
    let mut sol = Solution::new();
    sol.k = k;
    sol.hits = hits;
    sol.frame = Some(TurnedBox { centre: frame.centre + k, ..frame });
    sol
}
