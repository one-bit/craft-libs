//! The caps of one query, charged in real units before the work they stand for, and the report of what a query did.

/// The caps of one query. Every unit is charged before the work it stands for. A query that meets a cap stops that piece of work, sets the
/// cap's bit in [`Work::cut`], and says `truncated`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Budget {
    /// Bucket entries visited in one window. A query gathers at most five windows: one round the subject, and one for each direction an
    /// equal-gap row is read in.
    pub window_visits: u32,
    /// Objects kept near the subject, for lines across a reach, places and sizes.
    pub candidates: u32,
    /// The subject's own places pulled with a box; the rest are left out.
    pub own: u32,
    /// Sorted-array entries walked past a bisection (reach filters, near-equal groups).
    pub line_steps: u32,
    /// Combinations of directions solved for a move, each candidate fallback the rounding needs counted.
    pub solves: u32,
    /// Places weighed.
    pub points: u32,
    /// Objects whose outline is asked for.
    pub outlines: u32,
    /// Path segments read, curve pieces estimated before flattening, and pairs of pieces crossed.
    pub pieces: u32,
    /// Pairs of outlines crossed.
    pub pairs: u32,
    /// Objects in equal-gap rows.
    pub row: u32,
    /// Marks emitted.
    pub marks: u32,
    /// The detail source's own units.
    pub detail: u64,
    /// How far a flattened curve may leave the curve, in document units.
    pub flatness: f64,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            window_visits: 4_096,
            candidates: 64,
            own: 32,
            line_steps: 4_096,
            solves: 512,
            points: 4_096,
            outlines: 16,
            pieces: 16_384,
            pairs: 64,
            row: 256,
            marks: 64,
            detail: u64::MAX,
            flatness: crate::osnap::DEFAULT_FLATNESS,
        }
    }
}

/// Which caps a query met.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cuts(pub u16);

impl Cuts {
    pub const WINDOW_VISITS: u16 = 1;
    pub const CANDIDATES: u16 = 1 << 1;
    pub const OWN: u16 = 1 << 2;
    pub const LINE_STEPS: u16 = 1 << 3;
    pub const SOLVES: u16 = 1 << 4;
    pub const POINTS: u16 = 1 << 5;
    pub const OUTLINES: u16 = 1 << 6;
    pub const PIECES: u16 = 1 << 7;
    pub const PAIRS: u16 = 1 << 8;
    pub const ROW: u16 = 1 << 9;
    pub const MARKS: u16 = 1 << 10;
    pub const DETAIL: u16 = 1 << 11;

    /// Whether the cap was met.
    pub fn has(&self, bit: u16) -> bool {
        self.0 & bit != 0
    }

    /// Whether any cap was met.
    pub fn any(&self) -> bool {
        self.0 != 0
    }
}

/// Why a query gave no snap at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The zoom is not finite or not above 0.
    Zoom,
    /// The subject, a step, an angle or a ratio is not a finite number.
    NotFinite,
    /// A setting is not usable (a tolerance that is not finite, an angle period not above 0).
    Settings,
}

/// Which indexes this query made (each is made the first time a query needs it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Made(pub u8);

impl Made {
    pub const OBJECT_LINES: u8 = 1;
    pub const BUCKETS: u8 = 1 << 1;
    pub const LENGTHS: u8 = 1 << 2;
    pub const ANGLES: u8 = 1 << 3;
    pub const POINTS: u8 = 1 << 4;

    /// Whether the index was made.
    pub fn has(&self, bit: u8) -> bool {
        self.0 & bit != 0
    }
}

/// What a query did: the counts of the units each cap bounds, and whether any cap was met. All counters saturate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Work {
    /// Steps of the bisections that find a direction and a line in a sorted array.
    pub bisect_steps: u64,
    /// Entries walked past a bisection.
    pub line_steps: u64,
    /// Bucket entries visited, a shape in several buckets once for each, over all windows.
    pub visited: u64,
    /// Objects looked at, each once in a window.
    pub examined: u64,
    /// Objects kept near the subject.
    pub candidates: u64,
    /// Places weighed.
    pub points: u64,
    /// Path segments read, curve pieces estimated, and pieces crossed.
    pub pieces: u64,
    /// The straight pieces the curves were estimated to flatten into, worked out before any was flattened.
    pub flattened: u64,
    /// Pairs of outlines crossed.
    pub pairs: u64,
    /// Objects whose outline was asked for.
    pub outlines: u64,
    /// Combinations solved.
    pub solves: u64,
    /// Objects in equal-gap rows.
    pub row: u64,
    /// Marks emitted.
    pub marks: u64,
    /// Own places pulled.
    pub own: u64,
    /// What the detail source charged.
    pub detail: u64,
    /// Marks the final check dropped because they did not hold. It must be 0.
    pub marks_dropped: u64,
    /// Which caps were met.
    pub cut: Cuts,
    /// Which indexes this query made.
    pub made: Made,
    /// Whether anything was left out: a cap was met.
    pub truncated: bool,
    /// Why nothing snapped, if nothing did.
    pub refused: Option<Refusal>,
}

impl Work {
    /// Charges `n` units of `counter` against `cap`. Returns whether they fit; when they do not, nothing is charged, the cap's bit is set and
    /// the work says `truncated`.
    pub fn charge(counter: &mut u64, cap: u64, n: u64, cuts: &mut Cuts, truncated: &mut bool, bit: u16) -> bool {
        if counter.saturating_add(n) > cap {
            cuts.0 |= bit;
            *truncated = true;
            return false;
        }
        *counter = counter.saturating_add(n);
        true
    }

    /// Notes that a cap was met without charging anything.
    pub fn cut(&mut self, bit: u16) {
        self.cut.0 |= bit;
        self.truncated = true;
    }
}

/// What a detail source charges its reading against.
pub struct Spend<'a> {
    pub(crate) work: &'a mut Work,
    pub(crate) budget: &'a Budget,
}

impl Spend<'_> {
    /// Charges `n` of the source's own units against [`Budget::detail`]. Returns whether they fit; the source stops when told no.
    pub fn detail(&mut self, n: u64) -> bool {
        let cap = self.budget.detail;
        Work::charge(&mut self.work.detail, cap, n, &mut self.work.cut, &mut self.work.truncated, Cuts::DETAIL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_charge_that_does_not_fit_is_refused_whole_and_says_so() {
        let mut work = Work::default();
        let budget = Budget { detail: 10, ..Budget::default() };
        let mut spend = Spend { work: &mut work, budget: &budget };
        assert!(spend.detail(6));
        assert!(!spend.detail(5), "5 more than the 4 left");
        assert!(spend.detail(4));
        assert!(!spend.detail(1));
        assert_eq!(work.detail, 10);
        assert!(work.truncated && work.cut.has(Cuts::DETAIL) && work.cut.any());
        let mut big = 0u64;
        let (mut cuts, mut t) = (Cuts::default(), false);
        assert!(!Work::charge(&mut big, u64::MAX, u64::MAX, &mut cuts, &mut t, 1) || big == u64::MAX);
    }

    #[test]
    fn counters_saturate_and_cuts_are_bits() {
        let mut c = u64::MAX - 1;
        let (mut cuts, mut t) = (Cuts::default(), false);
        assert!(!Work::charge(&mut c, u64::MAX, 5, &mut cuts, &mut t, Cuts::ROW) || c == u64::MAX);
        let mut w = Work::default();
        w.cut(Cuts::MARKS);
        assert!(w.truncated && w.cut.has(Cuts::MARKS) && !w.cut.has(Cuts::ROW));
        assert!(Made(Made::BUCKETS).has(Made::BUCKETS) && !Made(0).has(Made::BUCKETS));
    }
}
