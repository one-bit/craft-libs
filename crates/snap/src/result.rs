//! What a query gives: where the subject lands, what it snapped to, and what to draw.

use kurbo::{Line, Point, Vec2};

use crate::budget::Work;
use crate::frame::{Direction, Id, OwnOrBounds, Side, TurnedBox};
use crate::kind::Kind;

/// What each free degree snapped to.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub kind: Kind,
    pub id: Option<Id>,
    /// The direction pulled along; `None` for a place or an angle.
    pub direction: Option<Direction>,
    /// Which side of the subject was pulled, and whether it is one of its own sides or of the box round it.
    pub side: Option<(Side, OwnOrBounds)>,
    /// For an app's place: its number.
    pub number: Option<u32>,
    /// How far the subject was pulled, in document units (radians for an angle).
    pub distance: f64,
}

/// What to draw. All of it holds on the geometry the result returns.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// The final geometry lies on a line, drawn as `segment`. It covers the subject's side and every coincident target. `first` and `count`:
    /// the targets it stands for. `offset` is `direction.normal() · p` for the points `p` of the line.
    Line { direction: Direction, offset: f64, segment: Line, kinds: Vec<Kind>, first: Option<Id>, count: u32, hit: bool },
    /// A gap, drawn as `segment` from one side to the other across the middle of their overlap; `value` is its length. Gaps with one `chain`
    /// number run on from one another. `subject`: one of the subject's own gaps.
    Gap { direction: Direction, segment: Line, value: f64, chain: u16, subject: bool, hit: bool },
    /// The subject's final own width or height equals another object's: `side` is the subject's side, and `matched` the other's with its id.
    Size { value: f64, side: Line, matched: Option<(Id, Line)>, count: u32, hit: bool },
    /// The final angle equals another object's (`matched`), or is a whole number of steps.
    Angle { pivot: Point, angle: f64, matched: Option<Id>, step: Option<f64>, hit: bool },
    /// A place of the subject lies on a target place.
    Place { at: Point, kind: Kind, id: Option<Id>, number: Option<u32>, hit: bool },
}

/// What a query gives.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapped {
    /// What to add to the subject (box, point, nudge).
    pub delta: Vec2,
    /// The final box (box, edges, nudge). Its angle is the subject's, unchanged.
    pub frame: Option<TurnedBox>,
    /// The final point.
    pub point: Option<Point>,
    /// The final angle, radians.
    pub angle: Option<f64>,
    /// What each free degree snapped to: at most two lines (one per degree), or one place, or one angle; one per moving side for a resize.
    pub hits: Vec<Hit>,
    /// What to draw, all derived from the final geometry, in document coordinates.
    pub marks: Vec<Mark>,
    pub work: Work,
}

impl Snapped {
    pub(crate) fn empty(work: Work) -> Snapped {
        Snapped { delta: Vec2::ZERO, frame: None, point: None, angle: None, hits: Vec::new(), marks: Vec::new(), work }
    }
}

/// What a check found wrong.
#[derive(Clone, Debug, PartialEq)]
pub struct Broken {
    /// Which rule broke, in words.
    pub rule: &'static str,
    /// The mark or hit it concerns, as text.
    pub what: String,
}
