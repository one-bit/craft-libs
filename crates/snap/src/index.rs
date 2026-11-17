//! The indexes of a target set.
//!
//! * **Lines** sorted by direction, then by kind, then by offset, with equal places grouped into one entry that keeps the union of their
//!   spans and their count. The nearest line to a side is one of the two entries beside its insertion point, whatever the array holds.
//!   (Sorted arrays and the test that bisection equals a scan come from VectorCraft; see the README.)
//! * **A bucket grid** of boxes, visited from the middle of a window outward, so that when the visits run out it is the boxes farthest from the
//!   pointer that are left out, and not the ones under it. A box that covers more than [`MAX_BUCKETS_PER_BOX`] buckets is on a list every
//!   window looks at, each entry a visit. (The grid with a list of wide entries, and its test against a scan, follow PDFCraft's.)
//! * **Sorted arrays** of lengths and angles.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

use kurbo::{Point, Rect};

use crate::frame::{Coincide, Id, Side, fold};
use crate::kind::Kind;

/// The bits of the bisection that finds a place among `n` entries: the steps a search of `n` costs.
pub(crate) fn log_steps(n: usize) -> u64 {
    u64::from(usize::BITS - n.leading_zeros())
}

/// `angle` folded into the canonical range `[-eps, π - eps)`, and whether the canonical normal is the opposite of the normal at `angle`. An angle
/// just under a half turn is the one just over 0, with its offsets changed in sign.
pub(crate) fn canon(angle: f64, eps: f64) -> (f64, bool) {
    let (a, flipped) = fold(angle);
    if a > PI - eps { (a - PI, !flipped) } else { (a, flipped) }
}

/// A line before it is grouped: its canonical direction angle, its offset along that direction's normal, and the extent of the side along it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RawLine {
    pub angle: f64,
    pub offset: f64,
    pub lo: f64,
    pub hi: f64,
    pub kind: Kind,
    pub id: Option<Id>,
    /// Which side of its box the line is, when it is a side of one.
    pub side: Option<Side>,
}

/// The lines of one kind at one place in one direction.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Entry {
    pub offset: f64,
    /// The extent along the line, the union of every line grouped here. `lo > hi` for no extent (a line that runs on for ever).
    pub lo: f64,
    pub hi: f64,
    pub count: u32,
    pub id: Option<Id>,
    /// Where its members are in the set's `members`, when it holds more than one with a span.
    pub m0: u32,
    pub mn: u32,
}

/// One line of a group of coincident lines: its extent along the line.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Member {
    pub lo: f64,
    pub hi: f64,
    pub id: Option<Id>,
}

/// The lines of one kind in one direction, sorted by offset.
#[derive(Clone, Debug)]
pub(crate) struct Set {
    pub kind: Kind,
    pub side: Option<Side>,
    pub entries: Vec<Entry>,
    /// The members of the entries that hold more than one, each entry's sorted by where it starts along the line.
    pub members: Vec<Member>,
    /// For each member, the index within its entry of the member that reaches farthest of those up to it.
    pub argmax: Vec<u32>,
}

impl Set {
    /// The extent along the line of the member of an entry nearest to `ext`, the extent of the subject's side: the union of them all for an
    /// entry of one. Found by bisection in `log2(m)` among the `m` members.
    pub fn nearest_span(&self, e: &Entry, ext: (f64, f64)) -> (f64, f64) {
        let union = (e.lo, e.hi);
        if e.mn == 0 || ext.0.partial_cmp(&ext.1).is_none_or(|o| o.is_gt()) {
            return union;
        }
        let range = e.m0 as usize..(e.m0 + e.mn) as usize;
        let (Some(m), Some(am)) = (self.members.get(range.clone()), self.argmax.get(range)) else { return union };
        let (a, b) = ext;
        let r = m.partition_point(|x| x.lo <= b);
        let right = m.get(r).map(|x| (x.lo - b, *x));
        let left = r.checked_sub(1).and_then(|i| am.get(i)).and_then(|j| m.get(*j as usize)).map(|x| ((a - x.hi).max(0.0), *x));
        [left, right]
            .into_iter()
            .flatten()
            .min_by(|p, q| p.0.total_cmp(&q.0).then(p.1.id.cmp(&q.1.id)).then(p.1.lo.total_cmp(&q.1.lo)))
            .map_or(union, |(_, x)| (x.lo, x.hi))
    }

    /// The index of the first entry whose offset is not below `at`.
    pub fn insertion(&self, at: f64) -> usize {
        self.entries.partition_point(|e| e.offset < at)
    }

    /// The nearest entry to `at`: one of the two beside its insertion point. A tie goes to the lower id, then the lower offset.
    pub fn nearest(&self, at: f64) -> Option<usize> {
        let i = self.insertion(at);
        let before = i.checked_sub(1).filter(|j| self.entries.get(*j).is_some());
        let after = Some(i).filter(|j| self.entries.get(*j).is_some());
        match (before, after) {
            (Some(b), Some(a)) => {
                let (eb, ea) = (self.entries.get(b)?, self.entries.get(a)?);
                let (db, da) = ((at - eb.offset).abs(), (ea.offset - at).abs());
                if db < da || (db == da && id_before(eb.id, ea.id)) { Some(b) } else { Some(a) }
            }
            (b, a) => b.or(a),
        }
    }
}

/// Whether id `a` sorts before `b`, a missing id last.
pub(crate) fn id_before(a: Option<Id>, b: Option<Id>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x < y,
        (Some(_), None) => true,
        _ => false,
    }
}

/// One direction: the lines whose normals are within the tolerance of its first.
#[derive(Clone, Debug)]
pub(crate) struct Dir {
    /// The canonical angle of the normal, that of the first line of the group.
    pub angle: f64,
    pub sets: Vec<Set>,
}

impl Dir {
    /// The lines of `kind` in this direction.
    #[cfg(test)]
    pub fn set(&self, kind: Kind) -> Option<&Set> {
        let i = self.sets.partition_point(|s| s.kind < kind);
        self.sets.get(i).filter(|s| s.kind == kind)
    }
}

/// Every direction of a set of lines, sorted by angle.
#[derive(Clone, Debug, Default)]
pub(crate) struct DirTable {
    pub dirs: Vec<Dir>,
    /// How many places (entries) the lines came to.
    pub places: u64,
}

fn raw_order(a: &RawLine, b: &RawLine) -> std::cmp::Ordering {
    a.angle
        .total_cmp(&b.angle)
        .then(a.kind.cmp(&b.kind))
        .then(a.side.cmp(&b.side))
        .then(a.offset.total_cmp(&b.offset))
        .then(a.id.cmp(&b.id))
        .then(a.lo.total_cmp(&b.lo))
        .then(a.hi.total_cmp(&b.hi))
}

fn within_group_order(a: &RawLine, b: &RawLine) -> std::cmp::Ordering {
    a.kind
        .cmp(&b.kind)
        .then(a.side.cmp(&b.side))
        .then(a.offset.total_cmp(&b.offset))
        .then(a.id.cmp(&b.id))
        .then(a.lo.total_cmp(&b.lo))
        .then(a.hi.total_cmp(&b.hi))
        .then(a.angle.total_cmp(&b.angle))
}

fn merge(into: &mut Entry, line: &RawLine) {
    into.lo = into.lo.min(line.lo);
    into.hi = into.hi.max(line.hi);
    into.count = into.count.saturating_add(1);
    if into.id.is_none() || id_before(line.id, into.id) {
        into.id = line.id.or(into.id);
    }
}

impl DirTable {
    /// Sorts the lines by direction, kind and offset, and groups the equal places. The result does not depend on the order the lines came in.
    pub fn build(mut raw: Vec<RawLine>, co: &Coincide) -> DirTable {
        raw.sort_by(raw_order);
        let mut table = DirTable::default();
        let mut start = 0;
        while start < raw.len() {
            let first = raw.get(start).map_or(0.0, |l| l.angle);
            let mut end = start;
            while raw.get(end).is_some_and(|l| l.angle - first <= co.angle) {
                end += 1;
            }
            if end == start {
                end += 1;
            }
            if let Some(group) = raw.get_mut(start..end) {
                group.sort_by(within_group_order);
                table.dirs.push(Dir { angle: first, sets: group_sets(group, co) });
            }
            start = end;
        }
        table.places = table.dirs.iter().flat_map(|d| d.sets.iter()).map(|s| s.entries.len() as u64).sum();
        table
    }

    /// The direction within the tolerance of `angle` that is nearest to it, if any. The half turn wraps: a direction just under it is the
    /// one just over 0. The search is one bisection among the directions and two checks beside the ends.
    pub fn find(&self, angle: f64, co: &Coincide) -> Option<&Dir> {
        let (a, _) = canon(angle, co.angle);
        let i = self.dirs.partition_point(|d| d.angle < a);
        let last = self.dirs.len().checked_sub(1)?;
        let near = |d: &Dir| {
            let x = (a - d.angle).rem_euclid(PI);
            x.min(PI - x)
        };
        [i.checked_sub(1), Some(i.min(last)), Some(0), Some(last)]
            .into_iter()
            .flatten()
            .filter_map(|j| self.dirs.get(j).map(|d| (near(d), j, d)))
            .filter(|(x, _, _)| *x <= co.angle)
            .min_by(|p, q| p.0.total_cmp(&q.0).then(p.1.cmp(&q.1)))
            .map(|(_, _, d)| d)
    }

    /// The directions, for a test.
    #[cfg(test)]
    pub fn all_entries(&self) -> Vec<(f64, Kind, Entry)> {
        self.dirs.iter().flat_map(|d| d.sets.iter().flat_map(move |s| s.entries.iter().map(move |e| (d.angle, s.kind, *e)))).collect()
    }
}

/// Closes the last entry of a set: its members, when it holds more than one, are sorted along the line and kept with the farthest reach so far.
fn close_entry(set: &mut Set, pending: &mut Vec<Member>) {
    if pending.len() > 1 {
        pending.sort_by(|a, b| a.lo.total_cmp(&b.lo).then(a.hi.total_cmp(&b.hi)).then(a.id.cmp(&b.id)));
        let (m0, mn) = (set.members.len(), pending.len());
        let mut best = 0usize;
        for (i, m) in pending.iter().enumerate() {
            if pending.get(best).is_none_or(|b| m.hi > b.hi) {
                best = i;
            }
            set.argmax.push(best as u32);
        }
        set.members.extend(pending.iter().copied());
        if let (Some(e), Ok(a), Ok(b)) = (set.entries.last_mut(), u32::try_from(m0), u32::try_from(mn)) {
            (e.m0, e.mn) = (a, b);
        }
    }
    pending.clear();
}

/// The sets of one direction's lines, which are sorted by kind, side, then offset.
fn group_sets(group: &[RawLine], co: &Coincide) -> Vec<Set> {
    let mut sets: Vec<Set> = Vec::new();
    let mut pending: Vec<Member> = Vec::new();
    for line in group {
        if sets.last().is_none_or(|s| s.kind != line.kind || s.side != line.side) {
            if let Some(last) = sets.last_mut() {
                close_entry(last, &mut pending);
            }
            sets.push(Set { kind: line.kind, side: line.side, entries: Vec::new(), members: Vec::new(), argmax: Vec::new() });
        }
        let Some(set) = sets.last_mut() else { continue };
        match set.entries.last_mut() {
            Some(e) if co.same(e.offset, line.offset) => merge(e, line),
            _ => {
                close_entry(set, &mut pending);
                set.entries.push(Entry { offset: line.offset, lo: line.lo, hi: line.hi, count: 1, id: line.id, m0: 0, mn: 0 });
            }
        }
        if line.lo <= line.hi {
            pending.push(Member { lo: line.lo, hi: line.hi, id: line.id });
        }
    }
    if let Some(last) = sets.last_mut() {
        close_entry(last, &mut pending);
    }
    sets
}

/// The most buckets one box may cover before it goes on the list every window looks at.
pub(crate) const MAX_BUCKETS_PER_BOX: i64 = 16;

/// The most buckets in each direction a window may cover: a wider window is cut to the part round its middle.
pub(crate) const MAX_WINDOW_BUCKETS: i64 = 48;

/// The farthest bucket number, in each direction.
const MAX_BUCKET: f64 = 2_147_483_648.0;

/// One box in a grid, with the position the owner holds it at.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cell {
    pub bounds: Rect,
    pub id: Id,
    pub index: u32,
}

/// What a window found.
#[derive(Debug, Default)]
pub(crate) struct Gather<'a> {
    /// The boxes that meet the window, the nearest to its middle first, at most the limit.
    pub items: Vec<&'a Cell>,
    /// How many boxes were looked at, each once.
    pub examined: u64,
    /// How many bucket entries were visited, a box in several buckets once for each.
    pub visited: u64,
    /// Whether the window held more than the limit, or the visits ran out before the buckets did.
    pub cut: bool,
}

/// A bucket grid of boxes in a frame whose origin is `origin`.
#[derive(Debug)]
pub(crate) struct Grid {
    cell: f64,
    items: Vec<Cell>,
    buckets: HashMap<(i64, i64), Vec<u32>>,
    big: Vec<u32>,
}

fn bucket_of(v: f64, cell: f64) -> i64 {
    // A number that is not finite is the farthest bucket (NaN is 0): the cast saturates and never panics.
    (v / cell).floor().clamp(-MAX_BUCKET, MAX_BUCKET) as i64
}

/// The buckets at the distance `r` (the larger of the two differences) from `(cx, cy)`: that one alone for 0, else the sides of the square
/// round it, each bucket once. At most `8 r` of them.
fn ring(cx: i64, cy: i64, r: i64) -> impl Iterator<Item = (i64, i64)> {
    let across = (cx - r..=cx + r).flat_map(move |bx| if r == 0 { vec![(bx, cy)] } else { vec![(bx, cy - r), (bx, cy + r)] });
    let down = (cy - r + 1..=cy + r - 1).flat_map(move |by| [(cx - r, by), (cx + r, by)]);
    across.chain(down)
}

fn distance_to(r: &Rect, p: Point) -> f64 {
    let dx = (r.x0 - p.x).max(0.0).max(p.x - r.x1);
    let dy = (r.y0 - p.y).max(0.0).max(p.y - r.y1);
    dx.hypot(dy)
}

fn meets(a: &Rect, b: &Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}

impl Grid {
    /// A grid of these boxes. The size of a bucket follows the boxes: twice the median of their longer sides (1 when there is none).
    pub fn build(items: Vec<Cell>) -> Grid {
        let mut sides: Vec<f64> = items.iter().map(|i| i.bounds.width().max(i.bounds.height())).filter(|s| s.is_finite() && *s > 0.0).collect();
        sides.sort_by(f64::total_cmp);
        let median = sides.get(sides.len() / 2).copied().unwrap_or(0.5);
        let cell = (2.0 * median).max(f64::MIN_POSITIVE.sqrt());
        let mut grid = Grid { cell, items: Vec::new(), buckets: HashMap::new(), big: Vec::new() };
        for item in items {
            grid.insert(item);
        }
        grid
    }

    fn insert(&mut self, item: Cell) {
        let Ok(number) = u32::try_from(self.items.len()) else { return };
        let (x0, x1) = (bucket_of(item.bounds.x0, self.cell), bucket_of(item.bounds.x1, self.cell));
        let (y0, y1) = (bucket_of(item.bounds.y0, self.cell), bucket_of(item.bounds.y1, self.cell));
        let count = (x1 - x0 + 1).saturating_mul(y1 - y0 + 1);
        if count > MAX_BUCKETS_PER_BOX {
            self.big.push(number);
        } else {
            for bx in x0..=x1 {
                for by in y0..=y1 {
                    self.buckets.entry((bx, by)).or_default().push(number);
                }
            }
        }
        self.items.push(item);
    }

    /// How many boxes are in the grid.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Every box, for a test.
    #[cfg(test)]
    pub fn cells(&self) -> &[Cell] {
        &self.items
    }

    /// How many boxes are on the list every window looks at.
    #[cfg(test)]
    pub fn big_len(&self) -> usize {
        self.big.len()
    }

    /// The boxes that meet `window`, the nearest to its middle first (then the lower id), at most `limit`, with at most `visits` bucket entries
    /// visited. The buckets are visited in rings round the middle one.
    pub fn gather(&self, window: &Rect, limit: usize, visits: u64) -> Gather<'_> {
        let mut found = Found { visited: 0, examined: 0, cut: false, seen: HashSet::new(), hits: Vec::new(), cap: visits, window: *window };
        let mut more = self.big.iter().all(|n| self.visit(*n, &mut found));
        if more && [window.x0, window.y0, window.x1, window.y1].iter().all(|v| v.is_finite()) {
            more = self.rings(window, &mut found);
        }
        let _ = more;
        let mid = Point::new(0.5 * (window.x0 + window.x1), 0.5 * (window.y0 + window.y1));
        let mut hits: Vec<(f64, &Cell)> =
            found.hits.iter().map(|n| self.items.get(*n as usize)).filter_map(|c| c.map(|c| (distance_to(&c.bounds, mid), c))).collect();
        hits.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)).then(a.1.index.cmp(&b.1.index)));
        let cut = found.cut || hits.len() > limit;
        hits.truncate(limit);
        Gather { items: hits.into_iter().map(|(_, c)| c).collect(), examined: found.examined, visited: found.visited, cut }
    }

    /// Visits the buckets of the window in rings round its middle. Returns false when the visits ran out.
    fn rings(&self, window: &Rect, found: &mut Found) -> bool {
        let (mut x0, mut x1) = (bucket_of(window.x0, self.cell), bucket_of(window.x1, self.cell));
        let (mut y0, mut y1) = (bucket_of(window.y0, self.cell), bucket_of(window.y1, self.cell));
        for (lo, hi) in [(&mut x0, &mut x1), (&mut y0, &mut y1)] {
            if *hi - *lo + 1 > MAX_WINDOW_BUCKETS {
                let mid = *lo + (*hi - *lo) / 2;
                *lo = mid - MAX_WINDOW_BUCKETS / 2;
                *hi = mid + MAX_WINDOW_BUCKETS / 2 - 1;
            }
        }
        let (cx, cy) = (x0 + (x1 - x0) / 2, y0 + (y1 - y0) / 2);
        let rings = (cx - x0).max(x1 - cx).max(cy - y0).max(y1 - cy);
        for r in 0..=rings {
            for (bx, by) in ring(cx, cy, r) {
                if bx < x0 || bx > x1 || by < y0 || by > y1 {
                    continue;
                }
                let Some(list) = self.buckets.get(&(bx, by)) else { continue };
                if !list.iter().all(|n| self.visit(*n, found)) {
                    return false;
                }
            }
        }
        true
    }

    /// One bucket entry: the box `number`, looked at once however many buckets it is in. False when the visits have run out.
    fn visit(&self, number: u32, found: &mut Found) -> bool {
        if found.visited >= found.cap {
            found.cut = true;
            return false;
        }
        found.visited += 1;
        if !found.seen.insert(number) {
            return true;
        }
        let Some(item) = self.items.get(number as usize) else { return true };
        found.examined += 1;
        if meets(&item.bounds, &found.window) {
            found.hits.push(number);
        }
        true
    }
}

/// What a gather collects as it visits.
struct Found {
    visited: u64,
    examined: u64,
    cut: bool,
    seen: HashSet<u32>,
    hits: Vec<u32>,
    cap: u64,
    window: Rect,
}

/// A length of one side of an object, for size match.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Length {
    pub length: f64,
    pub id: Id,
}

/// Lengths sorted by value, then id.
pub(crate) fn sorted_lengths(mut v: Vec<Length>) -> Vec<Length> {
    v.sort_by(|a, b| a.length.total_cmp(&b.length).then(a.id.cmp(&b.id)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(id: u64, x: f64, y: f64, w: f64, h: f64) -> Cell {
        Cell { bounds: Rect::new(x, y, x + w, y + h), id: Id(id), index: id as u32 }
    }

    fn raw(angle: f64, offset: f64, kind: Kind, id: u64) -> RawLine {
        RawLine { angle, offset, lo: -1.0, hi: 1.0, kind, id: Some(Id(id)), side: None }
    }

    #[test]
    fn a_query_finds_the_boxes_near_the_window_and_not_the_others() {
        let g = Grid::build(vec![cell(1, 0.0, 0.0, 1.0, 1.0), cell(2, 5.0, 5.0, 1.0, 1.0), cell(3, 0.5, 0.5, 0.2, 0.2)]);
        let found = g.gather(&Rect::new(0.4, 0.4, 0.6, 0.6), 10, 4096);
        let ids: Vec<u64> = found.items.iter().map(|i| i.id.0).collect();
        assert!(ids.contains(&1) && ids.contains(&3) && !ids.contains(&2));
        assert!(found.examined <= 2, "{}", found.examined);
    }

    #[test]
    fn a_window_with_more_boxes_than_the_limit_keeps_the_nearest() {
        let items: Vec<Cell> = (0..40).map(|i| cell(i, i as f64 * 0.05, 0.0, 0.01, 0.01)).collect();
        let g = Grid::build(items);
        let found = g.gather(&Rect::new(0.0, -0.5, 3.0, 0.5), 5, 4096);
        assert_eq!(found.items.len(), 5);
        assert!(found.cut);
        assert_eq!(found.items.first().map(|i| i.id.0), Some(30), "nearest to the middle of the window (x 1.5)");
    }

    /// Small boxes far from the windows below, so that the size of a bucket follows them and not the huge boxes.
    fn small(n: u64, first_id: u64) -> Vec<Cell> {
        (0..n).map(|i| cell(first_id + i, -500.0 + (i % 100) as f64 * 3.0, -500.0 + (i / 100) as f64 * 3.0, 1.0, 1.0)).collect()
    }

    #[test]
    fn a_huge_box_is_a_big_one_and_found_by_every_window() {
        let mut items = small(20, 10);
        items.extend([cell(1, -1000.0, -1000.0, 2000.0, 2000.0), cell(2, 5.0, 5.0, 1.0, 1.0)]);
        let g = Grid::build(items);
        assert_eq!(g.big_len(), 1);
        let found = g.gather(&Rect::new(100.0, 100.0, 101.0, 101.0), 10, 4096);
        assert_eq!(found.items.iter().map(|i| i.id.0).collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn many_big_boxes_are_all_found_and_each_is_a_visit() {
        let n = 300u64;
        let mut items = small(700, 1000);
        items.extend((0..n).map(|i| cell(i, -1000.0, -1000.0, 2000.0 + i as f64, 2000.0)));
        let g = Grid::build(items);
        assert_eq!((g.big_len(), g.len()), (n as usize, n as usize + 700));
        let found = g.gather(&Rect::new(100.0, 100.0, 101.0, 101.0), 400, 4096);
        assert_eq!(found.items.len(), n as usize);
        let mut items = small(6_000, 10_000);
        items.extend((0..5_000u64).map(|i| cell(i, -1000.0, -1000.0, 2000.0 + i as f64, 2000.0)));
        let crowded = Grid::build(items);
        assert_eq!(crowded.big_len(), 5_000);
        let found = crowded.gather(&Rect::new(100.0, 100.0, 101.0, 101.0), 64, 4096);
        assert!(found.visited <= 4096 && found.cut, "{found:?}");
    }

    #[test]
    fn a_crowded_corner_does_not_hide_the_box_under_the_pointer() {
        // The window is 21 buckets across. 4,300 small boxes sit in the bucket at its low corner, which a scan in order would visit first and use up the
        // visits; the one box at its middle must still be looked at.
        let mut items: Vec<Cell> = (0..4_300u64).map(|i| cell(i, -1.9 + (i % 10) as f64 * 0.001, -1.9, 0.1, 0.1)).collect();
        items.push(cell(9_999, 0.0, 0.0, 0.2, 0.2));
        let g = Grid::build(items);
        let found = g.gather(&Rect::new(-2.0, -2.0, 2.0, 2.0), 64, 4096);
        assert!(found.cut && found.visited <= 4096, "{}: {}", found.visited, found.cut);
        assert_eq!(found.items.first().map(|i| i.id.0), Some(9_999), "the nearest to the middle of the window is found, and first");
    }

    #[test]
    fn the_rings_round_a_bucket_are_each_bucket_once() {
        let mut seen = HashSet::new();
        let mut order = Vec::new();
        for r in 0..=6 {
            for c in ring(10, -3, r) {
                assert_eq!((c.0 - 10).abs().max((c.1 + 3).abs()), r, "{c:?}");
                assert!(seen.insert(c), "{c:?} twice");
                order.push(r);
            }
        }
        assert_eq!(seen.len(), 13 * 13);
        assert!(order.windows(2).all(|w| w[0] <= w[1]), "nearest rings first");
    }

    #[test]
    fn hostile_boxes_and_windows_do_not_overflow_or_loop() {
        let g = Grid::build(vec![cell(1, 1.0e300, 1.0e300, 1.0, 1.0), cell(2, -1.0e300, 0.0, 5.0, 1.0)]);
        assert_eq!(g.len(), 2);
        let found = g.gather(&Rect::new(-1.0e300, -1.0e300, 1.0e300, 1.0e300), 10, 4096);
        assert!(found.examined <= 2);
        let found = g.gather(&Rect::new(f64::NAN, 0.0, 1.0, 1.0), 10, 4096);
        assert!(found.items.len() <= 2);
        let found = g.gather(&Rect::new(0.0, 0.0, f64::INFINITY, 1.0), 10, 4096);
        assert!(found.items.len() <= 2);
    }

    #[test]
    fn a_window_wider_than_the_bound_costs_the_bound_not_the_width() {
        let g = Grid::build((0..10_000u64).map(|i| cell(i, i as f64 * 0.1, 0.0, 0.05, 0.05)).collect());
        let found = g.gather(&Rect::new(0.0, -1.0, 1000.0, 1.0), 64, 4096);
        assert!(found.examined <= 48 * 10 + 1, "examined {}", found.examined);
        assert!(found.items.len() <= 64);
    }

    #[test]
    fn the_grid_equals_a_scan() {
        let items: Vec<Cell> = (0..400u64)
            .map(|i| cell(i, ((i * 37) % 101) as f64 * 0.7, ((i * 53) % 89) as f64 * 0.6, 0.3 + (i % 5) as f64, 0.2 + (i % 3) as f64))
            .collect();
        let g = Grid::build(items.clone());
        for w in
            [Rect::new(10.0, 10.0, 20.0, 20.0), Rect::new(-5.0, -5.0, 2.0, 2.0), Rect::new(0.0, 0.0, 80.0, 60.0), Rect::new(30.0, 30.0, 30.0, 30.0)]
        {
            let found = g.gather(&w, 10_000, 1_000_000);
            let mut got: Vec<u64> = found.items.iter().map(|c| c.id.0).collect();
            got.sort_unstable();
            let mut want: Vec<u64> = items.iter().filter(|c| meets(&c.bounds, &w)).map(|c| c.id.0).collect();
            want.sort_unstable();
            if w.width() <= 47.0 * g.cell && w.height() <= 47.0 * g.cell {
                assert_eq!(got, want, "{w:?}");
            }
        }
        assert_eq!(g.cells().len(), 400);
    }

    #[test]
    fn directions_group_from_the_first_angle_and_do_not_drift() {
        let co = Coincide::default();
        let e = co.angle;
        // Angles 0, 0.6e, 1.2e, 1.8e apart: the first takes those within e of it (0 and 0.6e); the next group starts at 1.2e.
        let lines: Vec<RawLine> = [0.0, 0.6 * e, 1.2 * e, 1.8 * e].iter().enumerate().map(|(i, a)| raw(*a, 5.0, Kind::Guide, i as u64)).collect();
        let t = DirTable::build(lines, &co);
        assert_eq!(t.dirs.len(), 2);
        assert_eq!(t.dirs.first().map(|d| d.sets.first().map_or(0, |s| s.entries.first().map_or(0, |x| x.count))), Some(2));
        // A find within the tolerance of a direction's first angle gives it; farther gives none.
        assert!(t.find(0.0, &co).is_some());
        assert!(t.find(1.5e-9, &co).is_none() || t.find(1.5e-9, &co).is_some_and(|d| d.angle > 0.0));
        assert!(t.find(0.3, &co).is_none());
    }

    #[test]
    fn a_direction_just_under_a_half_turn_is_the_one_just_over_zero() {
        let co = Coincide::default();
        let (a, flipped) = canon(PI - 0.5e-9, co.angle);
        assert!(a < 0.0 && a > -co.angle && flipped);
        let lines = [raw(PI - 0.5e-9, -3.0, Kind::Guide, 1), raw(0.2e-9, 3.0, Kind::Guide, 2)];
        // The first is canonised to -0.5e-9 with its offset negated: the same line as the second, 3 along the normal.
        let canonical: Vec<RawLine> = lines
            .iter()
            .map(|l| {
                let (a, flipped) = canon(l.angle, co.angle);
                RawLine { angle: a, offset: if flipped { -l.offset } else { l.offset }, ..*l }
            })
            .collect();
        let t = DirTable::build(canonical, &co);
        assert_eq!(t.dirs.len(), 1);
        let entries = t.all_entries();
        assert_eq!(entries.len(), 1, "one place");
        assert_eq!(entries.first().map(|e| e.2.count), Some(2));
        for probe in [PI - 0.7e-9, 0.3e-9, PI + 0.1e-9, -0.1e-9] {
            assert!(t.find(probe, &co).is_some(), "{probe}");
        }
    }

    #[test]
    fn bisection_finds_the_nearest_line_as_a_scan_does() {
        let co = Coincide::default();
        let offsets: Vec<f64> = (0..200).map(|i| ((i * 7919) % 1009) as f64 * 0.37).collect();
        let lines: Vec<RawLine> = offsets.iter().enumerate().map(|(i, o)| raw(0.0, *o, Kind::Edge, i as u64)).collect();
        let t = DirTable::build(lines, &co);
        let d = t.find(0.0, &co).unwrap();
        let set = d.set(Kind::Edge).unwrap();
        assert!(d.set(Kind::Guide).is_none());
        for q in [-5.0, 0.0, 12.34, 100.0, 373.0, 1000.0] {
            let i = set.nearest(q).unwrap();
            let got = set.entries[i].offset;
            let want = offsets.iter().copied().min_by(|a, b| (a - q).abs().total_cmp(&(b - q).abs())).unwrap();
            assert_eq!((got - q).abs(), (want - q).abs(), "{q}");
        }
        assert!(Set { kind: Kind::Edge, side: None, entries: vec![], members: vec![], argmax: vec![] }.nearest(1.0).is_none());
        assert_eq!(log_steps(0), 0);
        assert_eq!(log_steps(1), 1);
        assert_eq!(log_steps(1_200_000), 21);
    }

    #[test]
    fn the_nearest_member_of_a_group_is_found_by_bisection_as_a_scan_finds_it() {
        let co = Coincide::default();
        // 40 lines at one place with spans of every length, then the extents of a subject's side along the line.
        let lines: Vec<RawLine> = (0..40u64)
            .map(|i| RawLine {
                angle: 0.0,
                offset: 2.0,
                lo: ((i * 37) % 101) as f64,
                hi: ((i * 37) % 101) as f64 + (i % 7) as f64,
                kind: Kind::Edge,
                id: Some(Id(i)),
                side: Some(Side::Min),
            })
            .collect();
        let t = DirTable::build(lines.clone(), &co);
        let set = t.dirs.first().and_then(|d| d.sets.first()).unwrap();
        assert_eq!((set.entries.len(), set.entries[0].mn), (1, 40));
        for ext in [(-5.0, -3.0), (0.0, 1.0), (50.0, 52.0), (60.5, 61.0), (200.0, 210.0), (30.0, 99.0)] {
            let got = set.nearest_span(&set.entries[0], ext);
            let dist = |lo: f64, hi: f64| {
                if hi < ext.0 {
                    ext.0 - hi
                } else if lo > ext.1 {
                    lo - ext.1
                } else {
                    0.0
                }
            };
            let best = lines.iter().map(|l| dist(l.lo, l.hi)).fold(f64::INFINITY, f64::min);
            assert_eq!(dist(got.0, got.1), best, "{ext:?}: {got:?}");
        }
        // An entry of one is its own span, and a subject with no extent gets the union.
        let one = DirTable::build(vec![lines[0]], &co);
        let s1 = one.dirs.first().and_then(|d| d.sets.first()).unwrap();
        assert_eq!(s1.nearest_span(&s1.entries[0], (5.0, 6.0)), (lines[0].lo, lines[0].hi));
        assert_eq!(set.nearest_span(&set.entries[0], (f64::INFINITY, f64::NEG_INFINITY)), (set.entries[0].lo, set.entries[0].hi));
    }

    #[test]
    fn equal_places_group_with_the_union_of_spans_and_the_lowest_id() {
        let co = Coincide::default();
        let mut lines: Vec<RawLine> = (0..100u64)
            .map(|i| RawLine { angle: 0.0, offset: 4.0, lo: i as f64, hi: i as f64 + 1.0, kind: Kind::Edge, id: Some(Id(100 - i)), side: None })
            .collect();
        lines.push(RawLine { angle: 0.0, offset: 4.0 + 1e-3, lo: 0.0, hi: 1.0, kind: Kind::Edge, id: None, side: None });
        let t = DirTable::build(lines.clone(), &co);
        let set = t.dirs.first().unwrap().set(Kind::Edge).unwrap();
        assert_eq!(set.entries.len(), 2);
        let e = set.entries[0];
        assert_eq!((e.count, e.lo, e.hi, e.id), (100, 0.0, 100.0, Some(Id(1))));
        lines.reverse();
        let u = DirTable::build(lines, &co);
        assert_eq!(format!("{:?}", u.dirs), format!("{:?}", t.dirs), "insertion order does not matter");
    }
}
