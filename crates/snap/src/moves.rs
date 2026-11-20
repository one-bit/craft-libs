// Priority only on ties, and the bias of a place against a pair of lines, follow EffectCraft's `better` and `POINT_BIAS`: storytold/effectcraft at
// 60e2cbbbe3d73c0a9074c25a1a615ccfec18721c, crates/engine/src/viewer.rs. Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors. MIT OR Apache-2.0.
// Changed: up to four directions are paired, constraints and roundings are solved inside the choice, and a candidate is checked on the geometry it lands on.

//! A move: a point, or a box whose sides snap in up to four directions (the page's and its own), with a constraint and a rounding solved
//! inside the snap.
//!
//! Each direction gives at most one best candidate (a few more under a rounding, which may take a candidate off its line). Two candidates
//! in directions that are not parallel fix the correction exactly; one candidate alone moves the subject along its line's normal; a
//! constraint leaves one free degree, along which each candidate is solved. The combination that snaps the most degrees wins, then the
//! settings choose. A candidate that the rounding takes off its line, or an equal gap that no longer repeats where the subject lands, is dropped and
//! the next is tried (for a gap, the next candidate of its direction stands in for it, alone and in the pairs it was in), so what is drawn is where
//! the subject lands.

use std::cmp::Ordering;

use kurbo::{Point, Vec2};

use crate::align::{Free, probe_cands};
use crate::budget::{Cuts, Work};
use crate::ctx::{Cand, Ctx, Probe, ProbeSide, probes, split_top, top};
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

/// How many more candidates a direction keeps behind its best once a spacing candidate is among its best. A spacing candidate is read again on the geometry
/// it lands on, where the row may be another and the gap may no longer repeat; the direction would then have nothing left to try, though a line or another
/// of its gaps holds there. They stand in for the kept one: alone, and, when the kept one fails where the subject lands, paired with what the other
/// directions kept.
const FALLBACKS: usize = 7;

/// The candidates that stand behind the best of a direction: the best `FALLBACKS` of the rest, best first.
fn fallbacks(rest: Vec<Cand>, choose: Choose, co: &crate::frame::Coincide) -> Vec<Cand> {
    top(rest, FALLBACKS, choose, co)
}

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
    /// 0 for a combination of what the directions kept, 1 once a fallback is in it: of two that are as good, the first is tried first.
    tier: u8,
    /// The directions (their places among the probes) of `a` and of `b`.
    dirs: (usize, usize),
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

/// A pair of candidates in two directions that are not parallel, as a combination: within the larger reach; and where both kinds have none, within a few
/// times the farther of the two pulls (a pair of directions nearly parallel throws the box without end).
fn pair_combo(a: &Cand, b: &Cand, (i, j): (usize, usize), probes: &[Probe], tier: u8) -> Option<Combo> {
    let (pa, pb) = (probes.get(i)?, probes.get(j)?);
    let k = pair_k(a, b, pa.normal, pb.normal)?;
    let reach = a.reach.max(b.reach);
    let limit = if reach.is_finite() { reach } else { crate::ctx::MAX_AMPLIFICATION * a.t.max(b.t) };
    (k.hypot() <= limit).then_some(Combo { degrees: 2, k, a: *a, b: Some(*b), tier, dirs: (i, j) })
}

fn combos(lists: &[Vec<Cand>], spares: &[Vec<Cand>], probes: &[Probe], free: Free) -> Vec<Combo> {
    let mut out = Vec::new();
    for (i, (list, spare)) in lists.iter().zip(spares).enumerate() {
        let Some(pi) = probes.get(i) else { continue };
        for (tier, cands) in [(0, list), (1, spare)] {
            for a in cands {
                let k = match free {
                    Free::Normal => pi.normal * a.d,
                    Free::Along(e) => e * a.d,
                };
                out.push(Combo { degrees: 1, k, a: *a, b: None, tier, dirs: (i, i) });
            }
        }
    }
    if !matches!(free, Free::Normal) {
        return out;
    }
    for (i, la) in lists.iter().enumerate() {
        for (j, lb) in lists.iter().enumerate().skip(i + 1) {
            for a in la {
                for b in lb {
                    out.extend(pair_combo(a, b, (i, j), probes, 0));
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
    y.degrees.cmp(&x.degrees).then(rule).then(x.tier.cmp(&y.tier)).then(ids(x).cmp(&ids(y))).then(x.len().total_cmp(&y.len()))
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

/// The candidates each direction offers: the ones it kept, and the fallbacks behind them, best first.
struct Pool<'a> {
    probes: &'a [Probe],
    free: Free,
    lists: Vec<Vec<Cand>>,
    spares: Vec<Vec<Cand>>,
}

impl Pool<'_> {
    /// When `failed`, a candidate of direction `dir`, is the last the direction has offered and it has a fallback behind it, the fallback is offered
    /// too, and the combinations it makes with what the other directions offer are returned: a candidate that fails where the subject lands is
    /// replaced by the next one in reach, in the pairs it was in as in the single. (Pairs of fallbacks are not made before one has failed: a
    /// direction's other candidates are never offered beside the ones the directions kept, or they would take a result from them.)
    fn promote(&mut self, dir: usize, failed: &Cand) -> Vec<Combo> {
        if !matches!(self.free, Free::Normal) {
            return Vec::new();
        }
        let (Some(list), Some(spare)) = (self.lists.get_mut(dir), self.spares.get_mut(dir)) else { return Vec::new() };
        if list.last() != Some(failed) || spare.is_empty() {
            return Vec::new();
        }
        let next = spare.remove(0);
        list.push(next);
        let mut out = Vec::new();
        for (j, others) in self.lists.iter().enumerate().filter(|(j, _)| *j != dir) {
            for b in others {
                out.extend(if j < dir { pair_combo(b, &next, (j, dir), self.probes, 1) } else { pair_combo(&next, b, (dir, j), self.probes, 1) });
            }
        }
        out
    }
}

/// The first candidate a combination names (the one of the lower direction first) that does not hold where the subject lands, moved by `k` from where the
/// directions were solved, and its direction. Only the first is read: it is the one that is replaced, and what follows from it is read in its turn.
fn first_failure(cx: &mut Ctx<'_>, combo: &Combo, k: Vec2, spaced: Option<(&TurnedBox, &[Probe])>) -> Option<(Cand, usize)> {
    let named = [Some((combo.a, combo.dirs.0)), combo.b.map(|b| (b, combo.dirs.1))];
    named.into_iter().flatten().find(|(c, _)| !(holds(cx, c, k) && spacing_holds(cx, c, spaced, k)))
}

/// Chooses the combination, applies the rounding, and checks that every candidate it names still holds; if not, the next. A candidate that fails
/// where the subject lands is replaced by the next of its direction.
fn choose(
    cx: &mut Ctx<'_>,
    list: Vec<Combo>,
    pool: &mut Pool<'_>,
    fr: &Freedom,
    spaced: Option<&TurnedBox>,
    refp: Point,
    minp: Point,
) -> Option<(Vec2, Combo)> {
    let mut list = list;
    let cap = u64::from(cx.budget().solves);
    let spaced = spaced.map(|f| (f, pool.probes));
    loop {
        let i = best_combo(&list, cx.s.choose, &cx.co)?;
        if !Work::charge(&mut cx.work.solves, cap, 1, &mut cx.work.cut, &mut cx.work.truncated, Cuts::SOLVES) {
            return None;
        }
        let combo = list.swap_remove(i);
        let k = fr.base + combo.k;
        let k = round_k(&cx.q.rounding, refp, minp, k, fr.on);
        match first_failure(cx, &combo, k - fr.base, spaced) {
            None => return Some((k, combo)),
            Some((c, dir)) => list.extend(pool.promote(dir, &c)),
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
        if let Some(f) = spaced {
            cands.extend(crate::spacing::cands(cx, f, p, fr.free));
        }
        let (kept, rest) = split_top(cands, n, cx.s.choose, &cx.co);
        // Where a spacing candidate is among the best, the next candidates of the direction stand behind it, whatever they are: it may fail where the
        // subject lands.
        let spare = if kept.iter().any(|c| c.kind == Kind::Spacing) { fallbacks(rest, cx.s.choose, &cx.co) } else { Vec::new() };
        lists.push(kept);
        spares.push(spare);
    }
    let list = combos(&lists, &spares, ps, fr.free);
    let mut pool = Pool { probes: ps, free: fr.free, lists, spares };
    choose(cx, list, &mut pool, fr, spaced, refp, minp)
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

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use super::*;
    use crate::frame::{Coincide, Id};

    fn cand(kind: Kind, id: u64, d: f64, prio: u8, angle: f64) -> Cand {
        Cand { kind, id: Some(Id(id)), prio, reach: 5.0, side: Side::Min, own: OwnOrBounds::Bounds, angle, c: 0.0, s: 0.0, d, t: d.abs(), pt: 0 }
    }

    fn probes() -> Vec<Probe> {
        vec![
            Probe { angle: 0.0, normal: Vec2::new(1.0, 0.0), sides: Vec::new() },
            Probe { angle: FRAC_PI_2, normal: Vec2::new(0.0, 1.0), sides: Vec::new() },
        ]
    }

    fn ids(c: &Combo) -> (u64, Option<u64>) {
        (c.a.id.map_or(0, |i| i.0), c.b.and_then(|b| b.id).map(|i| i.0))
    }

    #[test]
    fn a_candidate_that_failed_brings_in_the_next_of_its_direction_in_the_pairs_it_was_in() {
        let ps = probes();
        let (a0, a1, a2) = (cand(Kind::Spacing, 1, 1.0, 1, 0.0), cand(Kind::Edge, 2, 2.0, 1, 0.0), cand(Kind::Guide, 3, 3.0, 1, 0.0));
        let b0 = cand(Kind::Edge, 4, 0.5, 1, FRAC_PI_2);
        let mut pool = Pool { probes: &ps, free: Free::Normal, lists: vec![vec![a0], vec![b0]], spares: vec![vec![a1, a2], Vec::new()] };
        // The failed one is the last its direction has offered: the next is offered, and pairs with what the other direction offers.
        let made = pool.promote(0, &a0);
        assert_eq!(made.len(), 1);
        let m = &made[0];
        assert_eq!((ids(m), m.degrees, m.tier, m.dirs), ((2, Some(4)), 2, 1, (0, 1)));
        assert_eq!(m.k, Vec2::new(2.0, 0.5), "the pair is solved for the two lines");
        assert_eq!(pool.lists[0], vec![a0, a1]);
        assert_eq!(pool.spares[0], vec![a2]);
        // The same candidate failing again (alone, after the pair) is no longer the last: nothing more is offered.
        assert!(pool.promote(0, &a0).is_empty());
        assert_eq!((pool.lists[0].len(), pool.spares[0].len()), (2, 1));
        // The one brought in fails in its turn: the next after it.
        let made = pool.promote(0, &a1);
        assert_eq!(made.iter().map(ids).collect::<Vec<_>>(), vec![(3, Some(4))]);
        assert_eq!((pool.lists[0].len(), pool.spares[0].len()), (3, 0));
        // Nothing is left behind it.
        assert!(pool.promote(0, &a2).is_empty());
    }

    #[test]
    fn a_fallback_pairs_with_every_candidate_the_other_directions_offer_with_the_lower_direction_first() {
        let ps = probes();
        let (a0, b0, b1, b2) = (
            cand(Kind::Edge, 1, 1.0, 1, 0.0),
            cand(Kind::Edge, 4, 0.5, 1, FRAC_PI_2),
            cand(Kind::Guide, 5, 0.7, 1, FRAC_PI_2),
            cand(Kind::Guide, 6, 0.9, 1, FRAC_PI_2),
        );
        // Direction 1 brings in b2 behind b1 (its last), and pairs it with both candidates direction 0 offers.
        let mut pool = Pool { probes: &ps, free: Free::Normal, lists: vec![vec![a0], vec![b0, b1]], spares: vec![Vec::new(), vec![b2]] };
        let made = pool.promote(1, &b1);
        assert_eq!(made.iter().map(|c| (ids(c), c.dirs)).collect::<Vec<_>>(), vec![((1, Some(6)), (0, 1))], "a is from the lower direction");
        assert_eq!(pool.lists[1], vec![b0, b1, b2]);
        // A candidate of the other direction pairs with each of this direction's: those it kept and those brought in.
        let mut pool = Pool {
            probes: &ps,
            free: Free::Normal,
            lists: vec![vec![a0], vec![b0, b1]],
            spares: vec![vec![cand(Kind::Guide, 2, 1.5, 1, 0.0)], Vec::new()],
        };
        let made = pool.promote(0, &a0);
        assert_eq!(made.iter().map(|c| (ids(c), c.dirs)).collect::<Vec<_>>(), vec![((2, Some(4)), (0, 1)), ((2, Some(5)), (0, 1))]);
    }

    #[test]
    fn nothing_is_brought_in_for_a_move_along_a_line_or_when_the_direction_has_nothing_behind_it() {
        let ps = probes();
        let (a0, a1) = (cand(Kind::Spacing, 1, 1.0, 1, 0.0), cand(Kind::Edge, 2, 2.0, 1, 0.0));
        let b0 = cand(Kind::Edge, 4, 0.5, 1, FRAC_PI_2);
        let mut along =
            Pool { probes: &ps, free: Free::Along(Vec2::new(1.0, 0.0)), lists: vec![vec![a0], vec![b0]], spares: vec![vec![a1], Vec::new()] };
        assert!(along.promote(0, &a0).is_empty());
        assert_eq!((along.lists[0].len(), along.spares[0].len()), (1, 1), "the spare stays: a move along a line makes no pairs");
        let mut bare = Pool { probes: &ps, free: Free::Normal, lists: vec![vec![a0], vec![b0]], spares: vec![Vec::new(), Vec::new()] };
        assert!(bare.promote(0, &a0).is_empty() && bare.promote(1, &b0).is_empty());
        // A direction that is not there is nothing to promote in.
        assert!(bare.promote(7, &a0).is_empty());
        // A candidate that is not the last of its direction (one the direction did not offer) brings nothing in.
        let mut other = Pool { probes: &ps, free: Free::Normal, lists: vec![vec![a0], vec![b0]], spares: vec![vec![a1], Vec::new()] };
        assert!(other.promote(0, &cand(Kind::Guide, 9, 1.0, 1, 0.0)).is_empty());
        assert_eq!(other.lists[0], vec![a0]);
    }

    #[test]
    fn the_combinations_are_what_each_direction_kept_alone_and_paired_and_its_fallbacks_alone() {
        let ps = probes();
        let (a0, a1) = (cand(Kind::Spacing, 1, 1.0, 1, 0.0), cand(Kind::Edge, 2, 2.0, 1, 0.0));
        let (b0, b1) = (cand(Kind::Edge, 4, 0.5, 1, FRAC_PI_2), cand(Kind::Guide, 5, 0.7, 1, FRAC_PI_2));
        let list = combos(&[vec![a0], vec![b0]], &[vec![a1], vec![b1]], &ps, Free::Normal);
        let seen: Vec<_> = list.iter().map(|c| (ids(c), c.degrees, c.tier, c.dirs)).collect();
        assert_eq!(
            seen,
            vec![
                ((1, None), 1, 0, (0, 0)),
                ((2, None), 1, 1, (0, 0)),
                ((4, None), 1, 0, (1, 1)),
                ((5, None), 1, 1, (1, 1)),
                ((1, Some(4)), 2, 0, (0, 1)),
            ],
            "no pair is made of a fallback"
        );
        assert_eq!(list[0].k, Vec2::new(1.0, 0.0));
        assert_eq!(list[2].k, Vec2::new(0.0, 0.5));
        // Along a line only the singles are made.
        let along = combos(&[vec![a0], vec![b0]], &[vec![a1], vec![b1]], &ps, Free::Along(Vec2::new(1.0, 0.0)));
        assert_eq!(along.len(), 4);
        assert!(along.iter().all(|c| c.degrees == 1));
        assert_eq!(along[0].k, Vec2::new(1.0, 0.0), "the way along the line");
    }

    #[test]
    fn a_pair_beyond_the_larger_reach_is_no_combination() {
        let ps = probes();
        let near = |d: f64, angle: f64| Cand { reach: 1.0, ..cand(Kind::Edge, 1, d, 1, angle) };
        assert!(pair_combo(&near(0.6, 0.0), &near(0.7, FRAC_PI_2), (0, 1), &ps, 0).is_some_and(|c| (c.degrees, c.tier) == (2, 0)));
        assert!(pair_combo(&near(0.8, 0.0), &near(0.7, FRAC_PI_2), (0, 1), &ps, 0).is_none(), "a pair 1.06 away from a reach of 1");
        assert!(pair_combo(&near(0.6, 0.0), &near(0.7, 0.0), (0, 0), &ps, 0).is_none(), "directions that are one direction fix nothing");
        assert!(pair_combo(&near(0.6, 0.0), &near(0.7, FRAC_PI_2), (0, 5), &ps, 0).is_none(), "a direction that is not there");
        // Where neither kind has a reach, a few times the farther pull is the limit.
        let free = |d: f64, angle: f64| Cand { reach: f64::INFINITY, ..cand(Kind::Grid, 1, d, 1, angle) };
        assert!(pair_combo(&free(1.0, 0.0), &free(1.0, FRAC_PI_2), (0, 1), &ps, 1).is_some_and(|c| c.tier == 1));
    }

    #[test]
    fn where_neither_kind_has_a_reach_a_pair_may_throw_the_box_eight_times_the_farther_pull_and_no_more() {
        // Directions turned by `theta` from each other, pulls of 2 and -2 that disagree: the pair is solved for a correction of 4 / theta (nearly).
        let free = |theta: f64| {
            let ps = vec![
                Probe { angle: 0.0, normal: Vec2::new(1.0, 0.0), sides: Vec::new() },
                Probe { angle: theta, normal: Vec2::new(theta.cos(), theta.sin()), sides: Vec::new() },
            ];
            let (a, b) = (
                Cand { reach: f64::INFINITY, ..cand(Kind::Grid, 1, 2.0, 1, 0.0) },
                Cand { reach: f64::INFINITY, ..cand(Kind::Grid, 2, -2.0, 1, theta) },
            );
            pair_combo(&a, &b, (0, 1), &ps, 0).map(|c| c.k.hypot())
        };
        // The limit is 8 times 2 = 16.
        assert!(free(0.5).is_some_and(|k| (k - 8.0).abs() < 0.3), "{:?}: inside it (a limit of 4 would refuse)", free(0.5));
        assert!(free(0.3).is_some_and(|k| k > 12.0 && k < 16.0), "{:?}: inside it (a limit of 10 would refuse)", free(0.3));
        assert!(free(0.2).is_none(), "{:?}: a correction of 20 is past 16", free(0.2));
    }

    #[test]
    fn of_two_combinations_as_good_the_one_of_what_a_direction_kept_goes_first_and_more_degrees_beat_both() {
        let co = Coincide::default();
        let (a, b) = (cand(Kind::Edge, 1, 1.0, 1, 0.0), cand(Kind::Edge, 1, 1.0, 1, 0.0));
        let single = |c: Cand, tier: u8| Combo { degrees: 1, k: Vec2::new(c.d, 0.0), a: c, b: None, tier, dirs: (0, 0) };
        let (kept, spare) = (single(a, 0), single(b, 1));
        for choose in [Choose::Nearest, Choose::Ranked] {
            assert_eq!(combo_order(&kept, &spare, choose, &co), Ordering::Less);
            assert_eq!(combo_order(&spare, &kept, choose, &co), Ordering::Greater);
            // A pair of a fallback beats a single of what was kept, whatever its length: it snaps more of the box.
            let pair = Combo { degrees: 2, k: Vec2::new(9.0, 9.0), a: b, b: Some(b), tier: 1, dirs: (0, 1) };
            assert_eq!(combo_order(&pair, &kept, choose, &co), Ordering::Less);
        }
        // A nearer fallback goes before a farther one that was kept: a direction's own order is the settings', and the tier breaks a tie only.
        let near = single(cand(Kind::Edge, 2, 0.4, 1, 0.0), 1);
        assert_eq!(combo_order(&near, &kept, Choose::Nearest, &co), Ordering::Less);
    }
    #[test]
    fn a_pair_is_a_combination_within_the_larger_of_the_two_reaches() {
        let ps = probes();
        let with_reach = |d: f64, angle: f64, reach: f64| Cand { reach, ..cand(Kind::Edge, 1, d, 1, angle) };
        // Pulls of 2 and 2 across: a correction of 2.83. Within 5 (the larger) but not within 1.
        for (ra, rb) in [(1.0, 5.0), (5.0, 1.0), (5.0, 5.0)] {
            assert!(pair_combo(&with_reach(2.0, 0.0, ra), &with_reach(2.0, FRAC_PI_2, rb), (0, 1), &ps, 0).is_some(), "reaches {ra} and {rb}");
        }
        assert!(pair_combo(&with_reach(2.0, 0.0, 2.5), &with_reach(2.0, FRAC_PI_2, 1.0), (0, 1), &ps, 0).is_none(), "2.83 is beyond 2.5 and 1");
    }

    #[test]
    fn a_direction_has_seven_fallbacks_the_best_of_the_rest_and_best_first() {
        let co = Coincide::default();
        let rest: Vec<Cand> = (0..10u64).rev().map(|i| cand(Kind::Edge, 100 + i, 1.0 + 0.1 * i as f64, 1, 0.0)).collect();
        let kept = fallbacks(rest, Choose::Nearest, &co);
        assert_eq!(kept.iter().map(|c| c.id.map(|i| i.0)).collect::<Vec<_>>(), (100..107).map(Some).collect::<Vec<_>>());
        assert_eq!(fallbacks(Vec::new(), Choose::Nearest, &co).len(), 0);
        assert_eq!(fallbacks(vec![cand(Kind::Edge, 1, 1.0, 1, 0.0)], Choose::Ranked, &co).len(), 1);
    }

    #[test]
    fn the_first_candidate_a_combination_names_that_does_not_hold_is_the_one_replaced() {
        use crate::budget::Budget;
        use crate::query::{Constraint, Query, Rounding};
        use crate::settings::Settings;
        use crate::targets::{Limits, TargetsBuilder};
        let t = TargetsBuilder::new(Limits::default()).build();
        let s = Settings::default();
        let q = Query {
            subject: Subject::Point { at: Point::ORIGIN, base: None },
            zoom: 1.0,
            settings: &s,
            constraint: Constraint::None,
            rounding: Rounding::None,
            lattices: &[],
            view: None,
            budget: Budget::default(),
        };
        let mut cx = Ctx::new(&t, &q);
        // Both sides are 1 short of their lines along their own directions: a moved by (1, 0) is on its line and b by (0, 1) is.
        let line = |kind: Kind, id: u64, angle: f64| Cand { c: 1.0, s: 0.0, ..cand(kind, id, 1.0, 1, angle) };
        let (a, b) = (line(Kind::Edge, 1, 0.0), line(Kind::Guide, 2, FRAC_PI_2));
        let pair = Combo { degrees: 2, k: Vec2::new(1.0, 1.0), a, b: Some(b), tier: 0, dirs: (3, 4) };
        let single = Combo { degrees: 1, k: Vec2::new(1.0, 0.0), a, b: None, tier: 0, dirs: (3, 3) };
        let failed = |cx: &mut Ctx<'_>, c: &Combo, k: Vec2| first_failure(cx, c, k, None).map(|(c, d)| (c.id.map(|i| i.0), d));
        assert_eq!(failed(&mut cx, &pair, Vec2::new(0.0, 0.0)), Some((Some(1), 3)), "both fail: the first is the one");
        assert_eq!(failed(&mut cx, &pair, Vec2::new(1.0, 0.0)), Some((Some(2), 4)), "a holds, b fails");
        assert_eq!(failed(&mut cx, &pair, Vec2::new(0.0, 1.0)), Some((Some(1), 3)), "a fails, b holds");
        assert_eq!(failed(&mut cx, &pair, Vec2::new(1.0, 1.0)), None);
        assert_eq!(failed(&mut cx, &single, Vec2::new(0.0, 0.0)), Some((Some(1), 3)));
        assert_eq!(failed(&mut cx, &single, Vec2::new(1.0, 0.0)), None);
    }
}
