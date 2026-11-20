// Sorted lines searched by bisection follow VectorCraft: storytold/vectorcraft at 9a01d266b0438f96608c9f75b51437f912587d22, crates/tools/src/guides/targets.rs.
// Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors. MIT OR Apache-2.0.
// The bucket grid with a list of wide entries and its test against a scan follow PDFCraft: storytold/pdfcraft at efd265563edfe1b5a3261b184fbc4c83f67f4364,
// crates/measure/src/snap.rs. Copyright (c) 2026 ArtCraft Team and the PdfCraft contributors. MIT OR Apache-2.0.
// Changed: per direction (any angle) and per kind, equal places grouped with their members, buckets visited from the middle outward, every cap counted.

//! The indexes of a target set.
//!
//! * **Lines** sorted by direction, then by kind, then by offset, with equal places grouped into one entry that keeps the union of their
//!   spans and their count. The nearest line to a side is one of the two entries beside its insertion point, whatever the array holds.
//!   (Sorted arrays and the test that bisection equals a scan come from VectorCraft; see the README.)
//! * **A bucket grid** of boxes, visited from the middle of a window outward, so that when the visits run out it is the boxes farthest from the
//!   pointer that are left out, and not the ones under it. A box that covers more than [`MAX_BUCKETS_PER_BOX`] buckets is put in a coarser grid
//!   instead, one whose buckets are sixteen times as wide, and so on up, so a window looks at the coarse buckets it meets and not at every wide box
//!   there is; the grids are visited from the finest up, so the small boxes under the pointer are looked at before the wide ones that cover it. Only
//!   a box too wide for the coarsest grid is on a list every window looks at, last, each entry a visit. (The grid with a list of wide entries, and its
//!   test against a scan, follow PDFCraft's.)
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

/// An extent along a line: where it starts and where it ends.
pub(crate) type Span = (f64, f64);

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

    /// What a mark that runs from the nearest target on each side of `ext` (the extent of the subject's side) reaches among the members of an
    /// entry: the span to the left of `ext` and the span to the right, each found by bisection in `log2(m)`.
    ///
    /// A member is on the left when it starts before `ext` and on the right when it ends past it; one inside `ext` is on neither. On a side, the
    /// members that overlap or touch `ext` are the nearest (all of them: their join is the span), else the member that comes nearest without
    /// reaching it (the ties joined). The members are sorted by where they start, with the farthest reach so far kept, so the first member of the
    /// left whose reach touches `ext`, the farthest reach of the left and the first member past `ext` are each one bisection.
    ///
    /// An entry of one (or none) is its own span on the left, to be told by where it lies; `ext` that is not a range gives the union.
    pub fn each_side_spans(&self, e: &Entry, ext: Span) -> (Option<Span>, Option<Span>) {
        let union = (e.lo <= e.hi).then_some((e.lo, e.hi));
        let (a, b) = ext;
        if e.mn == 0 || a.partial_cmp(&b).is_none_or(|o| o.is_gt()) {
            return (union, None);
        }
        let range = e.m0 as usize..(e.m0 + e.mn) as usize;
        let (Some(m), Some(am)) = (self.members.get(range.clone()), self.argmax.get(range)) else { return (union, None) };
        let reach = |i: usize| am.get(i).and_then(|k| m.get(*k as usize));
        // The members that start before `ext`: the one that reaches farthest decides whether any of them touches `ext`.
        let p = m.partition_point(|x| x.lo < a);
        let left = p.checked_sub(1).and_then(reach).and_then(|far| {
            if far.hi >= a {
                let j = am.get(..p)?.partition_point(|k| m.get(*k as usize).is_some_and(|x| x.hi < a));
                Some((m.get(j)?.lo, far.hi))
            } else {
                Some((far.lo, far.hi))
            }
        });
        // The members that start no later than `ext` ends: one that reaches past it is on the right, else the first that starts after it.
        let r = m.partition_point(|x| x.lo <= b);
        let right = r.checked_sub(1).and_then(reach).filter(|far| far.hi > b).and_then(|far| {
            let j = am.get(..r)?.partition_point(|k| m.get(*k as usize).is_some_and(|x| x.hi <= b));
            Some((m.get(j)?.lo, far.hi))
        });
        let right = right.or_else(|| {
            let first = m.get(r)?;
            let end = m.partition_point(|x| x.lo <= first.lo);
            Some((first.lo, m.get(end.checked_sub(1)?)?.hi))
        });
        (left, right)
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

/// The most buckets in all that a long thin window (a strip) may cover: its shorter side is cut to [`MAX_WINDOW_BUCKETS`] as any window's is, and
/// its longer side to as many as the same number of buckets in all allows, so that a window one bucket across is read as far as 2,304 along.
pub(crate) const MAX_STRIP_BUCKETS: i64 = MAX_WINDOW_BUCKETS * MAX_WINDOW_BUCKETS;

/// How a window past the bounds is cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// To [`MAX_WINDOW_BUCKETS`] in each direction.
    Block,
    /// To [`MAX_WINDOW_BUCKETS`] in the shorter direction and, in the longer, to what [`MAX_STRIP_BUCKETS`] leaves.
    Strip,
}

impl Shape {
    /// The most buckets one side of a window may cover when the other side covers `other`.
    fn side(self, other: i64) -> i64 {
        match self {
            Shape::Block => MAX_WINDOW_BUCKETS,
            Shape::Strip => (MAX_STRIP_BUCKETS / other.clamp(1, MAX_WINDOW_BUCKETS)).max(MAX_WINDOW_BUCKETS),
        }
    }
}

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

/// How many times as wide a bucket of the next coarser grid is.
const COARSEN: f64 = 16.0;

/// How many grids there are at most: a box that does not fit the coarsest in [`MAX_BUCKETS_PER_BOX`] buckets is on the list of the widest.
const MAX_LEVELS: usize = 14;

/// One grid of buckets, all of one width.
#[derive(Debug)]
struct Level {
    cell: f64,
    buckets: HashMap<(i64, i64), Vec<u32>>,
}

/// A bucket grid of boxes in a frame whose origin is `origin`, with coarser grids for the boxes too wide for it.
#[derive(Debug)]
pub(crate) struct Grid {
    cell: f64,
    items: Vec<Cell>,
    levels: Vec<Level>,
    big: Vec<u32>,
    /// How many boxes are in a grid coarser than the finest.
    coarse: usize,
}

fn bucket_of(v: f64, cell: f64) -> i64 {
    // A number that is not finite is the farthest bucket (NaN is 0): the cast saturates and never panics.
    (v / cell).floor().clamp(-MAX_BUCKET, MAX_BUCKET) as i64
}

/// The buckets at the distance `r` (the larger of the two differences) from `(cx, cy)`: that one alone for 0, else the sides of the square
/// round it, each bucket once. At most `8 r` of them.
#[cfg(test)]
fn ring(cx: i64, cy: i64, r: i64) -> impl Iterator<Item = (i64, i64)> {
    let across = (cx - r..=cx + r).flat_map(move |bx| if r == 0 { vec![(bx, cy)] } else { vec![(bx, cy - r), (bx, cy + r)] });
    let down = (cy - r + 1..=cy + r - 1).flat_map(move |by| [(cx - r, by), (cx + r, by)]);
    across.chain(down)
}

/// The buckets of [`ring`] that lie in the window `(x0, y0, x1, y1)` (bucket numbers, both ends in), in the same order, put in `out`: the cost of
/// a ring is what the window holds of it and not the size of the ring, so that a window one bucket across costs its length and not its square.
fn ring_in(cx: i64, cy: i64, r: i64, (x0, y0, x1, y1): (i64, i64, i64, i64), out: &mut Vec<(i64, i64)>) {
    let within = |v: i64, lo: i64, hi: i64| lo <= v && v <= hi;
    // The two rows of the ring (one for 0) and its two columns, each only when it is in the window, so that a ring wholly outside costs nothing.
    let (top, bottom) = (within(cy - r, y0, y1), r > 0 && within(cy + r, y0, y1));
    if top || bottom {
        for bx in (cx - r).max(x0)..=(cx + r).min(x1) {
            if top {
                out.push((bx, cy - r));
            }
            if bottom {
                out.push((bx, cy + r));
            }
        }
    }
    let (left, right) = (within(cx - r, x0, x1), within(cx + r, x0, x1));
    if left || right {
        for by in (cy - r + 1).max(y0)..=(cy + r - 1).min(y1) {
            if left {
                out.push((cx - r, by));
            }
            if right {
                out.push((cx + r, by));
            }
        }
    }
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
        let mut grid = Grid { cell, items: Vec::new(), levels: Vec::new(), big: Vec::new(), coarse: 0 };
        for item in items {
            grid.insert(item);
        }
        grid
    }

    fn insert(&mut self, item: Cell) {
        let Ok(number) = u32::try_from(self.items.len()) else { return };
        let mut cell = self.cell;
        let mut placed = false;
        for level in 0..MAX_LEVELS {
            if !cell.is_finite() {
                break;
            }
            let (x0, x1) = (bucket_of(item.bounds.x0, cell), bucket_of(item.bounds.x1, cell));
            let (y0, y1) = (bucket_of(item.bounds.y0, cell), bucket_of(item.bounds.y1, cell));
            let count = (x1 - x0 + 1).saturating_mul(y1 - y0 + 1);
            if count <= MAX_BUCKETS_PER_BOX {
                while self.levels.len() <= level {
                    let width = self.cell * COARSEN.powi(i32::try_from(self.levels.len()).unwrap_or(0));
                    self.levels.push(Level { cell: width, buckets: HashMap::new() });
                }
                if let Some(grid) = self.levels.get_mut(level) {
                    for bx in x0..=x1 {
                        for by in y0..=y1 {
                            grid.buckets.entry((bx, by)).or_default().push(number);
                        }
                    }
                    placed = true;
                    self.coarse += usize::from(level > 0);
                }
                break;
            }
            cell *= COARSEN;
        }
        if !placed {
            self.big.push(number);
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

    /// How many boxes are too wide for the finest grid: in a coarser one, or on the list every window looks at.
    #[cfg(test)]
    pub fn big_len(&self) -> usize {
        self.coarse + self.big.len()
    }

    /// How many boxes are on the list every window looks at.
    #[cfg(test)]
    pub fn listed_len(&self) -> usize {
        self.big.len()
    }

    /// The boxes that meet `window`, the nearest to its middle first (then the lower id), at most `limit`, with at most `visits` bucket entries
    /// visited. The buckets are visited in rings round the middle one.
    pub fn gather(&self, window: &Rect, limit: usize, visits: u64) -> Gather<'_> {
        self.gather_from(window, None, limit, visits)
    }

    /// The width of one bucket of the finest grid.
    pub fn cell(&self) -> f64 {
        self.cell
    }

    /// As [`Grid::gather`], with the buckets visited in rings round `near` (kept inside the window) and the boxes ranked by their distance to it,
    /// where `gather` takes the middle of the window. A window that is long and thin, or that looks one way from the subject, is read from its end.
    pub fn gather_from(&self, window: &Rect, near: Option<Point>, limit: usize, visits: u64) -> Gather<'_> {
        self.gather_shaped(window, near, limit, visits, Shape::Block)
    }

    /// As [`Grid::gather_from`], for a window that is long and thin: its longer side may cover as many buckets as [`MAX_STRIP_BUCKETS`] allows
    /// for its shorter (up to 2,304 for a window one bucket across), so that a line of places is read from end to end. A window that is not thin is
    /// cut as `gather_from` cuts it, and a longer one is cut to the buckets round `near`, and says so.
    pub fn gather_strip(&self, window: &Rect, near: Option<Point>, limit: usize, visits: u64) -> Gather<'_> {
        self.gather_shaped(window, near, limit, visits, Shape::Strip)
    }

    fn gather_shaped(&self, window: &Rect, near: Option<Point>, limit: usize, visits: u64, shape: Shape) -> Gather<'_> {
        let mut found = Found { visited: 0, examined: 0, cut: false, seen: HashSet::new(), hits: Vec::new(), cap: visits, window: *window };
        // The finest grid first, then each coarser one, then the boxes too wide for any: the small boxes under the pointer are looked at before the wide
        // ones that cover it, and a window meets the few coarse buckets it is over and not every wide box there is.
        let mut more = true;
        if [window.x0, window.y0, window.x1, window.y1].iter().all(|v| v.is_finite()) {
            more = self.levels.iter().all(|level| self.rings(level, window, near, shape, &mut found));
        }
        if more {
            more = self.big.iter().all(|n| self.visit(*n, &mut found));
        }
        let _ = more;
        let mid = near.unwrap_or_else(|| Point::new(0.5 * (window.x0 + window.x1), 0.5 * (window.y0 + window.y1)));
        let mut hits: Vec<(f64, &Cell)> =
            found.hits.iter().map(|n| self.items.get(*n as usize)).filter_map(|c| c.map(|c| (distance_to(&c.bounds, mid), c))).collect();
        hits.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)).then(a.1.index.cmp(&b.1.index)));
        let cut = found.cut || hits.len() > limit;
        hits.truncate(limit);
        Gather { items: hits.into_iter().map(|(_, c)| c).collect(), examined: found.examined, visited: found.visited, cut }
    }

    /// Visits the buckets of the window in one grid, in rings round its middle. Returns false when the visits ran out.
    fn rings(&self, level: &Level, window: &Rect, near: Option<Point>, shape: Shape, found: &mut Found) -> bool {
        let (mut x0, mut x1) = (bucket_of(window.x0, level.cell), bucket_of(window.x1, level.cell));
        let (mut y0, mut y1) = (bucket_of(window.y0, level.cell), bucket_of(window.y1, level.cell));
        let at = near.map(|p| (bucket_of(p.x, level.cell), bucket_of(p.y, level.cell)));
        let (across, down) = (x1 - x0 + 1, y1 - y0 + 1);
        for (lo, hi, round, cap) in [(&mut x0, &mut x1, at.map(|a| a.0), shape.side(down)), (&mut y0, &mut y1, at.map(|a| a.1), shape.side(across))] {
            if *hi - *lo + 1 > cap {
                // Some of the window is not looked at: the answer says so. The part kept is round the middle, or round `near` when there is one.
                found.cut = true;
                let mid = round.unwrap_or(*lo + (*hi - *lo) / 2);
                let start = (mid - cap / 2).max(*lo).min(*hi - (cap - 1));
                *lo = start;
                *hi = start + cap - 1;
            }
        }
        let (cx, cy) = match at {
            Some((bx, by)) => (bx.max(x0).min(x1), by.max(y0).min(y1)),
            None => (x0 + (x1 - x0) / 2, y0 + (y1 - y0) / 2),
        };
        let rings = (cx - x0).max(x1 - cx).max(cy - y0).max(y1 - cy);
        let mut ring = Vec::new();
        for r in 0..=rings {
            ring.clear();
            ring_in(cx, cy, r, (x0, y0, x1, y1), &mut ring);
            for at in &ring {
                let Some(list) = level.buckets.get(at) else { continue };
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

    #[test]
    fn an_angle_just_under_the_half_turn_folds_to_just_under_zero_and_the_one_on_the_edge_does_not() {
        let eps = 1.0e-9;
        let (a, flipped) = canon(PI - eps, eps);
        assert_eq!((a, flipped), (PI - eps, false), "exactly on the edge it stays");
        let (a, flipped) = canon(PI - 0.5 * eps, eps);
        assert!(a < 0.0 && flipped);
    }

    #[test]
    fn a_tie_goes_to_the_lower_id_wherever_the_two_lines_are_and_a_line_with_an_id_beats_one_without() {
        let co = Coincide::default();
        let set_of = |lines: Vec<RawLine>| DirTable::build(lines, &co).dirs.into_iter().next().and_then(|d| d.sets.into_iter().next()).unwrap();
        let id_at = |set: &Set, q: f64| set.nearest(q).and_then(|i| set.entries.get(i)).map(|e| e.id);
        // 3 below and 3 above the query at 0: the lower id wins whichever side it is on.
        let a = set_of(vec![raw(0.0, -3.0, Kind::Guide, 9), raw(0.0, 3.0, Kind::Guide, 2)]);
        assert_eq!(id_at(&a, 0.0), Some(Some(Id(2))));
        let b = set_of(vec![raw(0.0, -3.0, Kind::Guide, 2), raw(0.0, 3.0, Kind::Guide, 9)]);
        assert_eq!(id_at(&b, 0.0), Some(Some(Id(2))));
        // A line that has an id beats one that has none, on either side.
        let none = |offset: f64| RawLine { id: None, ..raw(0.0, offset, Kind::Guide, 0) };
        let c = set_of(vec![none(-3.0), raw(0.0, 3.0, Kind::Guide, 7)]);
        assert_eq!(id_at(&c, 0.0), Some(Some(Id(7))));
        let d = set_of(vec![raw(0.0, -3.0, Kind::Guide, 7), none(3.0)]);
        assert_eq!(id_at(&d, 0.0), Some(Some(Id(7))));
        assert!(id_before(Some(Id(1)), Some(Id(2))) && !id_before(Some(Id(2)), Some(Id(1))) && !id_before(Some(Id(1)), Some(Id(1))));
        assert!(id_before(Some(Id(1)), None) && !id_before(None, Some(Id(1))) && !id_before(None, None));
    }

    #[test]
    fn lines_at_one_place_are_named_by_the_lowest_id_whatever_order_they_came_in() {
        let co = Coincide::default();
        // The lines are a hair apart, inside the tolerance, so they sort by offset and not by id, and the lowest id is the last to come.
        for ids in [[5, 4, 3], [3, 4, 5], [4, 5, 3]] {
            let lines: Vec<RawLine> = ids.iter().enumerate().map(|(i, id)| raw(0.0, 2.0 + i as f64 * 2.0e-10, Kind::Guide, *id)).collect();
            let entries = DirTable::build(lines, &co).all_entries();
            assert_eq!(entries.len(), 1, "{ids:?}");
            assert_eq!((entries[0].2.id, entries[0].2.count), (Some(Id(3)), 3), "{ids:?}");
        }
        let mixed = [RawLine { id: None, ..raw(0.0, 2.0, Kind::Guide, 0) }, raw(0.0, 2.0 + 2.0e-10, Kind::Guide, 8)];
        assert_eq!(DirTable::build(mixed.to_vec(), &co).all_entries()[0].2.id, Some(Id(8)));
        let mixed = [raw(0.0, 2.0, Kind::Guide, 8), RawLine { id: None, ..raw(0.0, 2.0 + 2.0e-10, Kind::Guide, 0) }];
        assert_eq!(DirTable::build(mixed.to_vec(), &co).all_entries()[0].2.id, Some(Id(8)));
    }

    #[test]
    fn the_nearest_member_of_a_group_is_the_one_with_the_smallest_gap_to_the_extent() {
        let co = Coincide::default();
        let member = |id: u64, lo: f64, hi: f64| RawLine { lo, hi, ..raw(0.0, 1.0, Kind::Edge, id) };
        let t = DirTable::build(vec![member(1, 0.0, 10.0), member(2, 19.0, 30.0)], &co);
        let set = t.dirs[0].sets[0].clone();
        let e = set.entries[0];
        assert_eq!(e.count, 2);
        // The extent 15 to 16 is 5 from the first member and 3 from the second.
        assert_eq!(set.nearest_span(&e, (15.0, 16.0)), (19.0, 30.0));
        // 11 to 12 is 1 from the first and 7 from the second.
        assert_eq!(set.nearest_span(&e, (11.0, 12.0)), (0.0, 10.0));
        // Inside the first member's reach the gap is none.
        assert_eq!(set.nearest_span(&e, (4.0, 6.0)), (0.0, 10.0));
        // An extent that is not a range is the union.
        assert_eq!(set.nearest_span(&e, (16.0, 15.0)), (e.lo, e.hi));
        // Two members that end together: the first of them is the one kept.
        let t = DirTable::build(vec![member(1, 0.0, 10.0), member(2, 5.0, 10.0)], &co);
        let set = t.dirs[0].sets[0].clone();
        assert_eq!(set.nearest_span(&set.entries[0], (12.0, 13.0)), (0.0, 10.0));
    }

    #[test]
    fn the_members_nearest_on_each_side_are_found_by_bisection_as_a_scan_finds_them() {
        let co = Coincide::default();
        // A small lattice so that members touch the extent, end together and start together; every case is checked against a scan.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        for _ in 0..400 {
            let count = 2 + next(12);
            let lines: Vec<RawLine> = (0..count)
                .map(|i| {
                    let lo = next(24) as f64;
                    RawLine { angle: 0.0, offset: 2.0, lo, hi: lo + next(7) as f64, kind: Kind::Edge, id: Some(Id(i)), side: None }
                })
                .collect();
            let t = DirTable::build(lines.clone(), &co);
            let set = t.dirs.first().and_then(|d| d.sets.first()).unwrap();
            let e = set.entries[0];
            let a = next(30) as f64 - 3.0;
            let ext = (a, a + next(8) as f64);
            let (left, right) = set.each_side_spans(&e, ext);
            let side = |on: Vec<&RawLine>, gap: &dyn Fn(&RawLine) -> f64| {
                let least = on.iter().map(|l| gap(l)).fold(f64::INFINITY, f64::min);
                let tied: Vec<&&RawLine> = on.iter().filter(|l| gap(l) == least).collect();
                (!tied.is_empty())
                    .then(|| (tied.iter().map(|l| l.lo).fold(f64::INFINITY, f64::min), tied.iter().map(|l| l.hi).fold(f64::NEG_INFINITY, f64::max)))
            };
            let want_left = side(lines.iter().filter(|l| l.lo < ext.0).collect(), &|l| (ext.0 - l.hi).max(0.0));
            let want_right = side(lines.iter().filter(|l| l.hi > ext.1).collect(), &|l| (l.lo - ext.1).max(0.0));
            assert_eq!((left, right), (want_left, want_right), "{ext:?} {lines:?}");
        }
        // An entry of one is its own span, to be told by where it lies, and an extent that is not a range gives the union.
        let one = DirTable::build(vec![raw(0.0, 2.0, Kind::Edge, 1)], &co);
        let s1 = one.dirs.first().and_then(|d| d.sets.first()).unwrap();
        assert_eq!(s1.each_side_spans(&s1.entries[0], (5.0, 6.0)), (Some((-1.0, 1.0)), None));
        let two = DirTable::build(
            vec![RawLine { lo: 0.0, hi: 1.0, ..raw(0.0, 2.0, Kind::Edge, 1) }, RawLine { lo: 5.0, hi: 6.0, ..raw(0.0, 2.0, Kind::Edge, 2) }],
            &co,
        );
        let s2 = two.dirs.first().and_then(|d| d.sets.first()).unwrap();
        let e = s2.entries[0];
        assert_eq!(s2.each_side_spans(&e, (3.0, 2.0)), (Some((0.0, 6.0)), None));
        assert_eq!(s2.each_side_spans(&e, (f64::NAN, 2.0)), (Some((0.0, 6.0)), None));
    }

    #[test]
    fn a_grid_gives_the_boxes_nearest_the_middle_of_the_window_first() {
        // The window's middle is at (5, 7): one box on each side of it, each a different distance away, and one on it.
        let items = vec![
            cell(1, 8.0, 6.5, 1.0, 1.0),
            cell(2, -1.0, 6.5, 1.0, 1.0),
            cell(3, 4.5, 11.0, 1.0, 1.0),
            cell(4, 4.5, -1.0, 1.0, 1.0),
            cell(5, 4.5, 6.5, 1.0, 1.0),
        ];
        let g = Grid::build(items);
        let found = g.gather(&Rect::new(-5.0, -3.0, 15.0, 17.0), 10, 1000);
        let ids: Vec<u64> = found.items.iter().map(|c| c.id.0).collect();
        assert_eq!(ids, vec![5, 1, 3, 2, 4]);
        assert!(!found.cut);
        // With room for fewer than there are, the cut is said, and not before.
        assert!(g.gather(&Rect::new(-5.0, -3.0, 15.0, 17.0), 4, 1000).cut);
        assert!(!g.gather(&Rect::new(-5.0, -3.0, 15.0, 17.0), 5, 1000).cut);
    }

    #[test]
    fn a_box_of_no_size_does_not_make_the_buckets_no_size() {
        let mut items: Vec<Cell> = (0..3).map(|i| cell(i, i as f64, 0.0, 0.0, 0.0)).collect();
        items.push(cell(9, 0.0, 5.0, 2.0, 1.0));
        let g = Grid::build(items);
        assert_eq!(g.cell, 4.0);
    }

    #[test]
    fn a_box_over_sixteen_buckets_is_on_the_list_every_window_reads_and_one_over_fewer_is_not() {
        // The buckets are 2 wide (the median of the sides is 1): a box 1 deep and 32 wide covers 17 of them across, and 30 wide covers 16.
        let mut items: Vec<Cell> = (0..20).map(|i| cell(i, 100.0 + i as f64 * 10.0, 100.0, 1.0, 1.0)).collect();
        items.push(cell(100, 0.1, 0.1, 30.0, 0.5));
        let g = Grid::build(items.clone());
        assert_eq!(g.big_len(), 0, "16 buckets across and one deep");
        items.push(cell(101, 0.1, 50.1, 33.0, 0.5));
        let g = Grid::build(items);
        assert_eq!(g.big_len(), 1, "17 across");
    }

    #[test]
    fn the_windows_of_every_shape_give_what_a_scan_gives() {
        let items: Vec<Cell> = (0..500u64)
            .map(|i| cell(i, ((i * 37) % 211) as f64 * 0.9 - 40.0, ((i * 53) % 193) as f64 * 0.8 - 30.0, 0.3 + (i % 5) as f64, 0.2 + (i % 4) as f64))
            .collect();
        let g = Grid::build(items.clone());
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        for _ in 0..300 {
            let (x, y) = (next() * 200.0 - 60.0, next() * 160.0 - 50.0);
            let (w, h) = (next() * 14.0 * g.cell, next() * 14.0 * g.cell);
            let window = Rect::new(x, y, x + w, y + h);
            let found = g.gather(&window, 10_000, 1_000_000);
            let mut got: Vec<u64> = found.items.iter().map(|c| c.id.0).collect();
            got.sort_unstable();
            let mut want: Vec<u64> = items.iter().filter(|c| meets(&c.bounds, &window)).map(|c| c.id.0).collect();
            want.sort_unstable();
            assert_eq!(got, want, "{window:?}");
            assert!(!found.cut, "{window:?}");
        }
    }

    #[test]
    fn a_window_past_the_bound_is_cut_to_the_buckets_round_its_middle_and_says_so() {
        // Boxes 1 wide, so buckets 2 wide: one box in each of 200 buckets along a row, and one in each of 200 along a column.
        let mut items: Vec<Cell> = (0..200u64).map(|i| cell(i, i as f64 * 2.0 + 0.5, 0.5, 1.0, 1.0)).collect();
        items.extend((0..200u64).map(|i| cell(1000 + i, 1000.5, i as f64 * 2.0 + 0.5, 1.0, 1.0)));
        let g = Grid::build(items);
        assert_eq!(g.cell, 2.0);
        // Buckets 0 to 99 across: 100 of them, cut to the 48 about the middle (buckets 25 to 72).
        let found = g.gather(&Rect::new(0.0, 0.0, 199.9, 1.0), 1000, 100_000);
        let mut ids: Vec<u64> = found.items.iter().map(|c| c.id.0).collect();
        ids.sort_unstable();
        assert_eq!(ids, (25..=72).collect::<Vec<u64>>());
        assert!(found.cut);
        // The same down a column.
        let found = g.gather(&Rect::new(1000.0, 0.0, 1001.0, 199.9), 1000, 100_000);
        let mut ids: Vec<u64> = found.items.iter().map(|c| c.id.0).collect();
        ids.sort_unstable();
        assert_eq!(ids, (1025..=1072).collect::<Vec<u64>>());
        assert!(found.cut);
        // Exactly 48 across is not cut, and 49 is.
        assert!(!g.gather(&Rect::new(0.0, 0.0, 95.9, 1.0), 1000, 100_000).cut);
        assert!(g.gather(&Rect::new(0.0, 0.0, 97.9, 1.0), 1000, 100_000).cut);
    }

    #[test]
    fn when_the_visits_run_out_the_buckets_kept_are_those_round_the_middle_of_the_window() {
        // Forty-one boxes in a row, one to a bucket; a window over all of them and room to look at five.
        let items: Vec<Cell> = (0..41u64).map(|i| cell(i, i as f64 * 2.0 + 0.5, 0.5, 1.0, 1.0)).collect();
        let g = Grid::build(items);
        assert_eq!(g.cell, 2.0);
        let found = g.gather(&Rect::new(0.0, 0.0, 81.9, 1.0), 100, 5);
        let mut ids: Vec<u64> = found.items.iter().map(|c| c.id.0).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![18, 19, 20, 21, 22]);
        assert!(found.cut);
        // The same down a column and with the window not on the origin.
        let items: Vec<Cell> = (0..41u64).map(|i| cell(i, -500.0 + 0.5, -300.0 + i as f64 * 2.0 + 0.5, 1.0, 1.0)).collect();
        let g = Grid::build(items);
        let found = g.gather(&Rect::new(-500.0, -300.0, -499.0, -218.1), 100, 5);
        let mut ids: Vec<u64> = found.items.iter().map(|c| c.id.0).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![18, 19, 20, 21, 22]);
    }

    #[test]
    fn a_thin_window_visits_only_the_buckets_in_it_and_not_the_rest_of_the_rings() {
        // Three rows of forty-one boxes, one to a bucket, and a window over the middle row only.
        let items: Vec<Cell> = (0..123u64).map(|i| cell(i, (i % 41) as f64 * 2.0 + 0.5, (i / 41) as f64 * 2.0 + 0.5, 1.0, 1.0)).collect();
        let g = Grid::build(items);
        assert_eq!(g.cell, 2.0);
        let found = g.gather(&Rect::new(0.0, 2.0, 81.9, 3.9), 1000, 100_000);
        assert_eq!(found.items.len(), 41);
        assert_eq!(found.visited, 41, "no bucket outside the window is looked in");
        assert_eq!(found.examined, 41, "and each box in them once");
        assert!(!found.cut);
        // The same for a thin column.
        let found = g.gather(&Rect::new(40.0, 0.0, 41.9, 5.9), 1000, 100_000);
        assert_eq!((found.items.len(), found.visited, found.examined), (3, 3, 3));
    }

    #[test]
    fn the_cell_of_a_grid_is_twice_the_median_of_the_longer_sides_and_one_for_places_with_no_size() {
        // Boxes 1 wide: buckets of 2. Boxes 3 high and 1 wide: of 6. Places that are points: of 1.
        assert_eq!(Grid::build(vec![cell(1, 0.0, 0.0, 1.0, 1.0), cell(2, 5.0, 5.0, 1.0, 1.0)]).cell(), 2.0);
        assert_eq!(Grid::build(vec![cell(1, 0.0, 0.0, 1.0, 3.0), cell(2, 5.0, 5.0, 1.0, 3.0)]).cell(), 6.0);
        assert_eq!(Grid::build(vec![cell(1, 0.0, 0.0, 0.0, 0.0), cell(2, 5.0, 5.0, 0.0, 0.0)]).cell(), 1.0);
    }

    #[test]
    fn a_window_read_from_one_end_gives_the_boxes_nearest_that_end_and_a_window_read_from_its_middle_is_as_it_was() {
        // A row of 200 boxes, one to a bucket of 2.
        let items: Vec<Cell> = (0..200u64).map(|i| cell(i, i as f64 * 2.0 + 0.5, 0.5, 1.0, 1.0)).collect();
        let g = Grid::build(items);
        let ids = |f: &Gather<'_>| f.items.iter().map(|c| c.id.0).collect::<Vec<u64>>();
        let window = Rect::new(0.0, 0.0, 95.9, 1.0);
        // From the middle, as before: the five nearest the middle of buckets 0 to 47.
        let mid = g.gather(&window, 5, 100_000);
        assert_eq!(ids(&mid), vec![23, 24, 22, 25, 21]);
        // From the left end: the five nearest it, whatever is at the middle.
        let left = g.gather_from(&window, Some(Point::new(0.0, 0.5)), 5, 100_000);
        assert_eq!(ids(&left), vec![0, 1, 2, 3, 4]);
        assert!(left.cut, "more than the limit met the window");
        // From the right end.
        let right = g.gather_from(&window, Some(Point::new(95.9, 0.5)), 5, 100_000);
        assert_eq!(ids(&right), vec![47, 46, 45, 44, 43]);
        // With few visits the buckets read are those round `near`, so the box at `near` is not the one left out.
        let few = g.gather_from(&window, Some(Point::new(0.0, 0.5)), 5, 3);
        assert_eq!(ids(&few), vec![0, 1, 2]);
        assert!(few.cut && few.visited == 3);
        // A `near` outside the window is clamped into it for the buckets, and the boxes are still ranked by their distance to it.
        let outside = g.gather_from(&window, Some(Point::new(-50.0, 0.5)), 3, 100_000);
        assert_eq!(ids(&outside), vec![0, 1, 2]);
    }

    #[test]
    fn a_window_too_wide_is_cut_to_the_buckets_at_its_near_end_and_never_past_its_own_ends() {
        let items: Vec<Cell> = (0..200u64).map(|i| cell(i, i as f64 * 2.0 + 0.5, 0.5, 1.0, 1.0)).collect();
        let g = Grid::build(items);
        let window = Rect::new(0.0, 0.0, 199.9, 1.0);
        let ids = |f: &Gather<'_>| {
            let mut v: Vec<u64> = f.items.iter().map(|c| c.id.0).collect();
            v.sort_unstable();
            v
        };
        // Near the left end: buckets 0 to 47, not buckets before 0 (there are none) nor a middle part.
        let left = g.gather_from(&window, Some(Point::new(1.0, 0.5)), 1000, 100_000);
        assert_eq!(ids(&left), (0..48).collect::<Vec<u64>>());
        assert!(left.cut);
        // Near the right end: the last 48 buckets, 52 to 99, and nothing past the window's end.
        let right = g.gather_from(&window, Some(Point::new(199.0, 0.5)), 1000, 100_000);
        assert_eq!(ids(&right), (52..100).collect::<Vec<u64>>());
        // Near the middle: the 48 about it, as `gather` does.
        let middle = g.gather_from(&window, Some(Point::new(100.0, 0.5)), 1000, 100_000);
        assert_eq!(ids(&middle).len(), 48);
        assert_eq!(ids(&g.gather(&window, 1000, 100_000)), (25..=72).collect::<Vec<u64>>());
    }

    #[test]
    fn a_ring_cut_to_a_window_is_the_ring_with_the_buckets_outside_it_left_out_in_the_same_order() {
        // Every window of up to 4 by 4 buckets about the origin, every centre in it, every ring to past the window's size: a window that holds none of
        // a ring gives nothing, and one that holds part of it gives that part in the order the whole ring has.
        let mut seen = 0;
        for (x0, x1) in [(-1, 1), (0, 3), (-3, 0), (0, 0), (-2, 1)] {
            for (y0, y1) in [(-1, 1), (0, 0), (-2, 2), (1, 3)] {
                for cx in x0..=x1 {
                    for cy in y0..=y1 {
                        for r in 0..=7 {
                            let mut got = Vec::new();
                            ring_in(cx, cy, r, (x0, y0, x1, y1), &mut got);
                            let want: Vec<(i64, i64)> = ring(cx, cy, r).filter(|(bx, by)| (x0..=x1).contains(bx) && (y0..=y1).contains(by)).collect();
                            assert_eq!(got, want, "ring {r} about ({cx}, {cy}) in x {x0}..{x1}, y {y0}..{y1}");
                            seen += want.len();
                        }
                    }
                }
            }
        }
        assert!(seen > 1000, "{seen}");
        // The cost: a ring wholly outside a window gives nothing, and one beside a one-bucket window gives its two buckets only.
        let mut out = Vec::new();
        ring_in(0, 0, 500, (-3, 0, 3, 0), &mut out);
        assert!(out.is_empty(), "{out:?}");
        ring_in(0, 0, 3, (-3, 0, 3, 0), &mut out);
        assert_eq!(out, vec![(-3, 0), (3, 0)]);
    }

    #[test]
    fn the_longer_side_of_a_strip_is_what_the_bucket_count_allows_and_a_block_is_cut_to_forty_eight_each_way() {
        for other in [1, 2, 3, 24, 47, 48, 49, 1_000_000, 0, -5] {
            assert_eq!(Shape::Block.side(other), 48, "{other}");
        }
        let side = |other| Shape::Strip.side(other);
        assert_eq!((side(1), side(2), side(3), side(4), side(24)), (2_304, 1_152, 768, 576, 96));
        assert_eq!((side(47), side(48), side(49), side(1_000_000)), (49, 48, 48, 48), "never less than 48");
        assert_eq!((side(0), side(-5)), (2_304, 2_304), "a side of no buckets is one");
    }

    /// A row of boxes one to a bucket of 2 along y in 0 to 2, and a second row in the next bucket up.
    fn rows(n: u64) -> Grid {
        let mut items: Vec<Cell> = (0..n).map(|i| cell(i, i as f64 * 2.0 + 0.5, 0.5, 1.0, 1.0)).collect();
        items.extend((0..n).map(|i| cell(100_000 + i, i as f64 * 2.0 + 0.5, 2.5, 1.0, 1.0)));
        Grid::build(items)
    }

    fn ids(f: &Gather<'_>) -> Vec<u64> {
        let mut v: Vec<u64> = f.items.iter().map(|c| c.id.0).collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn a_strip_is_read_end_to_end_where_a_block_of_the_same_size_is_cut() {
        let g = rows(1_000);
        assert_eq!(g.cell, 2.0);
        // 500 buckets along and one across (y 0 to 1.9 is bucket 0): every box of the first row, and the row above is not in the window.
        let window = Rect::new(0.0, 0.0, 999.9, 1.9);
        let strip = g.gather_strip(&window, Some(Point::new(0.0, 0.5)), 10_000, 100_000);
        assert_eq!(ids(&strip), (0..500).collect::<Vec<u64>>());
        assert!(!strip.cut, "the strip is within its bound");
        assert_eq!(strip.visited, 500, "one bucket entry for each box, and none for the buckets outside the window");
        // A block is cut to 48 from `near`, as it was.
        let block = g.gather_from(&window, Some(Point::new(0.0, 0.5)), 10_000, 100_000);
        assert_eq!(ids(&block), (0..48).collect::<Vec<u64>>());
        assert!(block.cut);
        // The same window read from its right end, and from its middle.
        let right = g.gather_strip(&window, Some(Point::new(999.9, 0.5)), 10_000, 100_000);
        assert_eq!(ids(&right), (0..500).collect::<Vec<u64>>());
        let middle = g.gather_strip(&window, None, 10_000, 100_000);
        assert_eq!(ids(&middle), (0..500).collect::<Vec<u64>>());
    }

    #[test]
    fn a_strip_two_buckets_across_may_be_half_as_long_and_one_wider_still_a_third() {
        // 2,000 buckets along. One across: cut to 2,304 would not cut; two across (y 0 to 3.9): 1,152; three: 768.
        let mut items: Vec<Cell> = (0..2_000u64).map(|i| cell(i, i as f64 * 2.0 + 0.5, 0.5, 1.0, 1.0)).collect();
        items.extend((0..3u64).map(|i| cell(100_000 + i, 0.5, i as f64 * 2.0 + 0.5, 1.0, 1.0)));
        let g = Grid::build(items);
        let near = Some(Point::new(0.0, 0.5));
        let one = g.gather_strip(&Rect::new(0.0, 0.0, 3_999.9, 1.9), near, 10_000, 100_000);
        assert!(!one.cut && one.items.iter().filter(|c| c.id.0 < 2_000).count() == 2_000);
        let two = g.gather_strip(&Rect::new(0.0, 0.0, 3_999.9, 3.9), near, 10_000, 100_000);
        assert!(two.cut);
        assert_eq!(two.items.iter().filter(|c| c.id.0 < 2_000).map(|c| c.id.0).max(), Some(1_151), "buckets 0 to 1,151 of the row");
        let three = g.gather_strip(&Rect::new(0.0, 0.0, 3_999.9, 5.9), near, 10_000, 100_000);
        assert_eq!(three.items.iter().filter(|c| c.id.0 < 2_000).map(|c| c.id.0).max(), Some(767), "buckets 0 to 767");
        // 2,000 across and 49 down is a block: 48 each way, round `near`.
        let mut tall: Vec<Cell> = (0..2_000u64).map(|i| cell(i, i as f64 * 2.0 + 0.5, 0.5, 1.0, 1.0)).collect();
        tall.extend((0..60u64).map(|i| cell(100_000 + i, 0.5, i as f64 * 2.0 + 0.5, 1.0, 1.0)));
        let g = Grid::build(tall);
        let block = g.gather_strip(&Rect::new(0.0, 0.0, 3_999.9, 99.9), near, 10_000, 100_000);
        assert!(block.cut);
        assert_eq!(block.items.iter().filter(|c| c.id.0 < 2_000).map(|c| c.id.0).max(), Some(47));
        assert_eq!(block.items.iter().filter(|c| c.id.0 >= 100_000).map(|c| c.id.0).max(), Some(100_047));
    }

    #[test]
    fn a_strip_longer_than_its_bound_is_cut_to_the_buckets_at_its_near_end_and_says_so() {
        let g = rows(3_000);
        let window = Rect::new(0.0, 0.0, 5_999.9, 1.9);
        let left = g.gather_strip(&window, Some(Point::new(1.0, 0.5)), 10_000, 100_000);
        assert_eq!(ids(&left), (0..2_304).collect::<Vec<u64>>());
        assert!(left.cut);
        // From the far end: the last 2,304 buckets of the window (696 to 2,999), nothing past its end.
        let right = g.gather_strip(&window, Some(Point::new(5_999.0, 0.5)), 10_000, 100_000);
        assert_eq!(ids(&right), (696..3_000).collect::<Vec<u64>>());
        assert!(right.cut);
    }

    #[test]
    fn a_strip_a_trillion_long_costs_the_bound_and_says_so() {
        let (cut, visited, near, ring_cost) = finishes(|| {
            let g = rows(30);
            let strip = g.gather_strip(&Rect::new(-1.0e12, 0.0, 1.0e12, 1.9), Some(Point::new(5.0, 0.5)), 100, 100_000);
            let wide = g.gather_strip(&Rect::new(-1.0e12, -1.0e12, 1.0e12, 1.0e12), Some(Point::new(5.0, 0.5)), 100, 100_000);
            (strip.cut && wide.cut, strip.visited.max(wide.visited), strip.items.iter().any(|c| c.id.0 == 2), wide.items.len())
        });
        assert!(cut, "a window narrowed says so");
        assert!(visited <= 100 && near && ring_cost <= 100);
    }

    #[test]
    fn a_strip_that_is_not_finite_is_not_read() {
        let g = rows(10);
        let found = g.gather_strip(&Rect::new(f64::NAN, 0.0, 1.0, 1.0), None, 10, 1_000);
        assert!(found.items.len() <= 20);
        let found = g.gather_strip(&Rect::new(0.0, 0.0, f64::INFINITY, 1.0), None, 10, 1_000);
        assert!(found.items.len() <= 20);
    }

    /// Small boxes in a block about the origin, so that the buckets are 2 wide.
    fn block(first_id: u64) -> Vec<Cell> {
        (0..5_000u64).map(|i| cell(first_id + i, -150.0 + (i % 100) as f64 * 3.0, -75.0 + (i / 100) as f64 * 3.0, 1.0, 1.0)).collect()
    }

    #[test]
    fn wide_boxes_far_from_the_window_are_not_visited_and_do_not_hide_the_box_under_it() {
        // 4,200 boxes 40 wide, each at its own place well away from the pointer, and 5,000 small ones about it: the window at the pointer must look at the
        // small box under it, and what it pays is the buckets it is over, not the wide boxes there are.
        let mut items = block(10_000);
        items.push(cell(1, 0.0, 0.0, 1.0, 1.0));
        items.extend((0..4_200u64).map(|i| cell(100 + i, 1_000.0 + i as f64 * 50.0, 1_000.0 + i as f64 * 50.0, 40.0, 1.0)));
        let g = Grid::build(items);
        assert!(g.big_len() >= 4_200, "{}", g.big_len());
        let found = g.gather(&Rect::new(0.2, 0.2, 0.8, 0.8), 64, 4_096);
        assert!(found.items.iter().any(|c| c.id.0 == 1), "{found:?}");
        assert!(found.visited < 50, "the wide boxes far away cost nothing: {}", found.visited);
        assert!(!found.cut);
    }

    #[test]
    fn a_crowd_of_wide_boxes_over_the_pointer_does_not_hide_the_small_box_under_it() {
        let mut items = block(10_000);
        items.push(cell(1, 0.0, 0.0, 1.0, 1.0));
        items.extend((0..4_200u64).map(|i| cell(100 + i, -100.0, -100.0, 200.0 + i as f64 * 0.001, 200.0)));
        let g = Grid::build(items);
        let found = g.gather(&Rect::new(0.2, 0.2, 0.8, 0.8), 64, 4_096);
        assert!(found.items.iter().any(|c| c.id.0 == 1), "{found:?}");
        assert!(found.cut && found.visited <= 4_096, "the crowd uses up what is left and says so: {found:?}");
    }

    #[test]
    fn a_box_too_wide_for_every_grid_is_on_the_list_and_still_found() {
        let mut items = small(20, 10);
        items.push(cell(1, -1.0e300, -1.0e300, 2.0e300, 2.0e300));
        items.push(cell(2, 5.0, 5.0, 1.0, 1.0));
        let g = Grid::build(items);
        assert_eq!(g.listed_len(), 1);
        let found = g.gather(&Rect::new(5.2, 5.2, 5.4, 5.4), 10, 4_096);
        let mut ids: Vec<u64> = found.items.iter().map(|c| c.id.0).collect();
        ids.sort_unstable();
        assert_eq!(ids, [1, 2]);
    }

    #[test]
    fn a_wide_box_is_in_the_grid_that_holds_it_in_sixteen_buckets_or_fewer() {
        let items: Vec<Cell> = (0..20).map(|i| cell(i, 100.0 + i as f64 * 10.0, 100.0, 1.0, 1.0)).collect();
        // Buckets 2 wide: widths 30 (16 buckets), 33 (17), 500 and 5,000,000 each in a coarser grid, and a window in each is found.
        for (id, width) in [(100u64, 30.0), (101, 33.0), (102, 500.0), (103, 5.0e6), (104, 2.0e13)] {
            let mut all = items.clone();
            all.push(cell(id, 0.1, 0.1, width, 0.5));
            let g = Grid::build(all);
            assert_eq!(g.listed_len(), 0, "{width}");
            let at = (0.1 + width * 0.999).min(1.0e13);
            let found = g.gather(&Rect::new(at, 0.2, at + 0.1, 0.3), 10, 4_096);
            assert!(found.items.iter().any(|c| c.id.0 == id), "{width}: {found:?}");
        }
    }

    /// Runs `work` on its own thread and fails when it has not finished in ten seconds, so that a loop that does not end is a failed test and not a
    /// hung one.
    fn finishes<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(work());
        });
        rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap_or_else(|_| panic!("the gather did not finish in ten seconds"))
    }

    #[test]
    fn a_window_a_trillion_wide_is_narrowed_to_its_middle_and_says_so() {
        let found = finishes(|| {
            let mut items: Vec<Cell> = small(30, 10);
            items.push(cell(1, 0.0, 0.0, 1.0, 1.0));
            items.push(cell(2, 1.0e11, 0.0, 1.0, 1.0));
            let g = Grid::build(items);
            // Wide across and tall, with the pointer box at the middle: the window is far wider than the buckets it may look in.
            let wide = g.gather(&Rect::new(-1.0e12, -1.0e12, 1.0e12, 1.0e12), 100, 100_000);
            let thin = g.gather(&Rect::new(-1.0e12, 0.2, 1.0e12, 0.8), 100, 100_000);
            (wide.cut, wide.visited, wide.items.iter().any(|c| c.id.0 == 1), thin.cut, thin.visited, thin.items.iter().any(|c| c.id.0 == 1))
        });
        let (cut, visited, middle, thin_cut, thin_visited, thin_middle) = found;
        assert!(cut && thin_cut, "a narrowed window says it was");
        assert!(visited <= 100 && thin_visited <= 100, "{visited} {thin_visited}");
        assert!(middle && thin_middle, "the box at the middle of the window is found");
    }
}
