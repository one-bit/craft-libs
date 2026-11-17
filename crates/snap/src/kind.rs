//! What a target is, and how far each kind pulls.

/// What a target is. Settings are given per kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    // Lines.
    /// A ruler guide.
    Guide,
    /// A page or artboard edge or centre line.
    Frame,
    /// A margin, column, bleed or baseline.
    Layout,
    /// Another object's side.
    Edge,
    /// Another object's centre line.
    Centre,
    /// A lattice.
    Grid,
    /// A second lattice: the ruler's ticks.
    Ruler,
    /// A line kind of the app's own.
    Line(u8),
    // Relations.
    /// A gap equal to another gap.
    Spacing,
    /// A width or height equal to another object's.
    Size,
    /// An angle equal to another object's.
    Angle,
    /// A whole number of angle steps.
    AngleStep,
    // Places.
    /// A place kind of the app's own (connection points, anchors).
    Point(u8),
    /// A corner of another object's box.
    Corner,
    /// The middle of a side of another object's box.
    SideMiddle,
    /// An end of a segment of an outline.
    Vertex,
    /// Halfway along a segment.
    Midpoint,
    /// The nearest point of an outline.
    Nearest,
    /// Where two outlines cross.
    Intersection,
    /// Where the line from a base point touches an outline.
    Tangent,
    /// The foot of the perpendicular from a base point.
    Perpendicular,
}

impl Kind {
    /// Whether this kind is a line.
    pub fn is_line(self) -> bool {
        matches!(self, Kind::Guide | Kind::Frame | Kind::Layout | Kind::Edge | Kind::Centre | Kind::Grid | Kind::Ruler | Kind::Line(_))
    }

    /// Whether this kind is a place.
    pub fn is_place(self) -> bool {
        matches!(
            self,
            Kind::Point(_)
                | Kind::Corner
                | Kind::SideMiddle
                | Kind::Vertex
                | Kind::Midpoint
                | Kind::Nearest
                | Kind::Intersection
                | Kind::Tangent
                | Kind::Perpendicular
        )
    }

    /// Whether a line of this kind has an edge or centre of its own that a side of the subject meets "like to like": edges meet edges and
    /// centres meet centres. Other lines (guides, frames, lattices) meet any side.
    pub fn has_side(self) -> bool {
        matches!(self, Kind::Edge | Kind::Centre)
    }

    /// The slot of the kind in a table of defaults: every `Line(_)` shares one, and every `Point(_)`.
    pub(crate) fn slot(self) -> usize {
        match self {
            Kind::Guide => 0,
            Kind::Frame => 1,
            Kind::Layout => 2,
            Kind::Edge => 3,
            Kind::Centre => 4,
            Kind::Grid => 5,
            Kind::Ruler => 6,
            Kind::Line(_) => 7,
            Kind::Spacing => 8,
            Kind::Size => 9,
            Kind::Angle => 10,
            Kind::AngleStep => 11,
            Kind::Point(_) => 12,
            Kind::Corner => 13,
            Kind::SideMiddle => 14,
            Kind::Vertex => 15,
            Kind::Midpoint => 16,
            Kind::Nearest => 17,
            Kind::Intersection => 18,
            Kind::Tangent => 19,
            Kind::Perpendicular => 20,
        }
    }
}

/// How far a target pulls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reach {
    /// This many screen units, at any zoom: a target pulls from the same distance on screen.
    Screen(f64),
    /// This many document units, at any zoom.
    Document(f64),
    /// From any distance: a grid that always snaps.
    Any,
}

impl Reach {
    /// Whether a target at document distance `d` pulls at `zoom` (screen units per document unit). The boundary is inclusive: a `Screen(px)`
    /// target pulls if and only if `d * zoom <= px`. Not a number: never.
    pub fn within(&self, d: f64, zoom: f64) -> bool {
        match *self {
            Reach::Screen(px) => px.is_finite() && px >= 0.0 && d * zoom <= px,
            Reach::Document(r) => r.is_finite() && r >= 0.0 && d <= r,
            Reach::Any => d.is_finite(),
        }
    }

    /// The reach in document units; `f64::INFINITY` for [`Reach::Any`], and 0 for a reach that is not usable.
    pub fn document(&self, zoom: f64) -> f64 {
        match *self {
            Reach::Screen(px) if px.is_finite() && px >= 0.0 => px / zoom,
            Reach::Document(r) if r.is_finite() && r >= 0.0 => r,
            Reach::Any => f64::INFINITY,
            _ => 0.0,
        }
    }
}

/// The rule for one kind: on or off, how far it pulls, its priority (lower wins).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KindRule {
    pub on: bool,
    pub reach: Reach,
    pub priority: u8,
}

const KINDS: usize = 21;

/// The rules of every kind. Every `Line(_)` shares a rule and every `Point(_)` does, unless the table holds one for that number.
#[derive(Clone, Debug, PartialEq)]
pub struct KindTable {
    rules: [KindRule; KINDS],
    custom: Vec<(Kind, KindRule)>,
}

impl KindTable {
    /// The rule of `kind`.
    pub fn rule(&self, kind: Kind) -> KindRule {
        if let Some((_, r)) = self.custom.iter().find(|(k, _)| *k == kind) {
            return *r;
        }
        self.rules.get(kind.slot()).copied().unwrap_or(KindRule { on: false, reach: Reach::Screen(0.0), priority: u8::MAX })
    }

    /// Sets the rule of `kind`. For `Line(n)` and `Point(n)` this is the rule of that number alone; the number-less rule is
    /// [`KindTable::set_all_lines`] and [`KindTable::set_all_points`].
    pub fn set(&mut self, kind: Kind, rule: KindRule) {
        if matches!(kind, Kind::Line(_) | Kind::Point(_)) {
            match self.custom.iter_mut().find(|(k, _)| *k == kind) {
                Some(slot) => slot.1 = rule,
                None => self.custom.push((kind, rule)),
            }
        } else if let Some(slot) = self.rules.get_mut(kind.slot()) {
            *slot = rule;
        }
    }

    /// Sets the rule every `Line(_)` takes that has none of its own.
    pub fn set_all_lines(&mut self, rule: KindRule) {
        if let Some(slot) = self.rules.get_mut(Kind::Line(0).slot()) {
            *slot = rule;
        }
    }

    /// Sets the rule every `Point(_)` takes that has none of its own.
    pub fn set_all_points(&mut self, rule: KindRule) {
        if let Some(slot) = self.rules.get_mut(Kind::Point(0).slot()) {
            *slot = rule;
        }
    }
}

impl Default for KindTable {
    fn default() -> Self {
        let near = |priority| KindRule { on: true, reach: Reach::Screen(6.0), priority };
        let any = |on, priority| KindRule { on, reach: Reach::Any, priority };
        let off = |priority| KindRule { on: false, reach: Reach::Screen(6.0), priority };
        let mut t = KindTable { rules: [near(0); KINDS], custom: Vec::new() };
        let table: [(Kind, KindRule); KINDS] = [
            (Kind::Intersection, near(0)),
            (Kind::Point(0), near(1)),
            (Kind::Corner, near(1)),
            (Kind::SideMiddle, near(1)),
            (Kind::Vertex, near(1)),
            (Kind::Guide, near(2)),
            (Kind::Midpoint, near(2)),
            (Kind::Edge, near(3)),
            (Kind::Centre, near(3)),
            (Kind::Frame, near(4)),
            (Kind::Layout, near(4)),
            (Kind::Line(0), near(4)),
            (Kind::Spacing, near(5)),
            (Kind::Size, near(6)),
            (Kind::Angle, near(6)),
            (Kind::Nearest, near(7)),
            (Kind::Grid, any(true, 8)),
            (Kind::AngleStep, any(true, 8)),
            (Kind::Ruler, any(false, 9)),
            (Kind::Tangent, off(2)),
            (Kind::Perpendicular, off(2)),
        ];
        for (kind, rule) in table {
            if let Some(slot) = t.rules.get_mut(kind.slot()) {
                *slot = rule;
            }
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boundary_is_inclusive_at_every_zoom() {
        for zoom in [0.5, 1.0, 2.0, 8.0] {
            let r = Reach::Screen(6.0);
            let d = 6.0 / zoom;
            assert!(r.within(d, zoom), "{zoom}: at the boundary");
            assert!(!r.within(d * (1.0 + 1.0e-12), zoom), "{zoom}: past it");
            assert_eq!(r.document(zoom), d);
        }
        assert!(Reach::Document(2.0).within(2.0, 100.0) && !Reach::Document(2.0).within(2.1, 0.001));
        assert!(Reach::Any.within(1.0e300, 1.0) && !Reach::Any.within(f64::NAN, 1.0));
        assert!(!Reach::Screen(f64::NAN).within(0.0, 1.0) && !Reach::Screen(-1.0).within(0.0, 1.0));
        assert_eq!(Reach::Any.document(1.0), f64::INFINITY);
        assert_eq!(Reach::Screen(f64::NAN).document(1.0), 0.0);
    }

    #[test]
    fn the_default_table_is_the_documented_one() {
        let t = KindTable::default();
        let on = |k| t.rule(k).on;
        let prio = |k| t.rule(k).priority;
        assert!(on(Kind::Intersection) && prio(Kind::Intersection) == 0 && !on(Kind::Ruler) && !on(Kind::Tangent));
        assert_eq!([prio(Kind::Corner), prio(Kind::Point(3)), prio(Kind::Vertex), prio(Kind::SideMiddle)], [1; 4]);
        assert_eq!([prio(Kind::Guide), prio(Kind::Midpoint)], [2, 2]);
        assert_eq!([prio(Kind::Edge), prio(Kind::Centre)], [3, 3]);
        assert_eq!([prio(Kind::Frame), prio(Kind::Layout), prio(Kind::Line(9))], [4, 4, 4]);
        assert_eq!([prio(Kind::Spacing), prio(Kind::Size), prio(Kind::Angle), prio(Kind::Nearest)], [5, 6, 6, 7]);
        assert_eq!(t.rule(Kind::Grid).reach, Reach::Any);
        assert_eq!(t.rule(Kind::Guide).reach, Reach::Screen(6.0));
    }

    #[test]
    fn a_numbered_kind_has_its_own_rule_or_the_shared_one() {
        let mut t = KindTable::default();
        t.set(Kind::Line(4), KindRule { on: false, reach: Reach::Document(1.0), priority: 77 });
        assert_eq!(t.rule(Kind::Line(4)).priority, 77);
        assert_eq!(t.rule(Kind::Line(5)).priority, 4);
        t.set_all_lines(KindRule { on: true, reach: Reach::Any, priority: 3 });
        assert_eq!((t.rule(Kind::Line(5)).priority, t.rule(Kind::Line(4)).priority), (3, 77));
        t.set(Kind::Line(4), KindRule { on: true, reach: Reach::Any, priority: 1 });
        assert_eq!(t.rule(Kind::Line(4)).priority, 1);
        t.set_all_points(KindRule { on: true, reach: Reach::Any, priority: 0 });
        assert_eq!(t.rule(Kind::Point(200)).priority, 0);
        assert!(
            Kind::Guide.is_line() && !Kind::Guide.is_place() && Kind::Point(1).is_place() && !Kind::Spacing.is_line() && !Kind::Spacing.is_place()
        );
        assert!(Kind::Edge.has_side() && !Kind::Guide.has_side());
    }
}
