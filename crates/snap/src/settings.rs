//! The policy each app passes: which kinds are on, how far they pull, which wins, and how the rarer guides behave.

use std::f64::consts::TAU;

use kurbo::Rect;

use crate::frame::Coincide;
use crate::kind::{KindTable, Reach};

/// How the candidates of one degree are chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choose {
    /// The nearest wins. Priority decides only between distances equal within `coincide`. The default.
    Nearest,
    /// The lowest priority number wins at any distance. The nearest decides within one priority.
    Ranked,
}

/// How a place competes with lines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Places {
    /// A place within reach beats any line, on both degrees of the move.
    First,
    /// A place wins unless a pair of lines fixing both degrees is nearer than it by more than `bias` of the place's reach. Under
    /// `Choose::Ranked` a pair wins when the worst priority number of its two lines is below the place's.
    Compete { bias: f64 },
}

/// Which sides of the subject meet which lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Matching {
    /// Any side of the subject meets any line. The default.
    AnyToAny,
    /// Edges meet edges and centres meet centres. Lines with no side (guides, frames, grids) meet any side.
    LikeToLike,
    /// The same side meets the same side: a low edge meets low edges, a high edge high edges, a middle middles. Lines with no side meet
    /// any side. A turned box's low side is the one low along its own axis, so this reads best between boxes turned the same way.
    SameSide,
}

/// What a nudge does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NudgeMode {
    /// To the first target strictly ahead within one step, else by the step. The default.
    Stop,
    /// To the next snap place ahead however far, else by one ruler tick, which the app gives.
    Next { tick: f64 },
    /// By the step exactly, with no snap. The marks are still those of the final box.
    Plain,
}

/// Which objects a size match compares with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SizeScope {
    /// Every object of the set. The default.
    Anywhere,
    /// Only objects whose box meets this rectangle (the view), in the app's frame.
    View(Rect),
}

/// What the box round a turned object is built from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxRound {
    /// The corners of its frame. The default.
    Corners,
    /// Its outline: the box the app gives for the subject, and, on demand, the box the detail source gives for the objects near, charged to the
    /// budget. An object with none falls back to its corners.
    Paths,
}

/// Which families of lines are on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frames {
    /// The page's directions: the box round the subject against every line in them.
    pub page: bool,
    /// The subject's own directions when it is turned.
    pub own: bool,
}

/// Which objects make up a row for equal gaps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpacingRow {
    /// The row is every object that overlaps the subject across. The default.
    Band,
    /// From each neighbour outward, the next object that overlaps the previous one across.
    Linked,
}

/// Which gaps of the row the subject's gap may repeat (besides the centred place between its two neighbours).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpacingGaps {
    /// Any gap between two objects of the row. The default.
    Row,
    /// Only each neighbour's gap to the object beyond it, on the same side.
    Neighbour,
}

/// How far along a direction the neighbours of an equal gap may be: the larger of `times` the subject's length and `at_least`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpacingReach {
    pub times: f64,
    pub at_least: f64,
}

/// Which lengths a size match compares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeAxes {
    /// Widths match widths and heights match heights.
    SameAxis,
    /// Either matches either.
    EitherAxis,
}

/// What marks are derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkRule {
    /// Every target the final geometry lies on, among the kinds on. The default.
    Coincident,
    /// Only what was snapped to.
    HitsOnly,
}

/// What an app passes with each query.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Per kind: on or off, how far it pulls, its priority (lower wins).
    pub kinds: KindTable,
    pub choose: Choose,
    pub places: Places,
    pub matching: Matching,
    pub frames: Frames,
    /// How far across another object may be and still be lined up with. `None`: anywhere.
    pub align_reach: Option<Reach>,
    pub spacing_reach: SpacingReach,
    pub spacing_row: SpacingRow,
    pub spacing_gaps: SpacingGaps,
    pub size_axes: SizeAxes,
    /// The turn after which two angles are the same: a full turn by default; a half or a quarter turn for boxes. The sorted angles of a
    /// target set are made for the first period asked for.
    pub angle_period: f64,
    pub marks: MarkRule,
    pub coincide: Coincide,
    /// A resize that drags a corner keeps its proportions without being asked (the app asks for a kept ratio on any other handle with
    /// `Constraint::Aspect`).
    pub corners_keep_proportions: bool,
    pub nudge: NudgeMode,
    pub size_scope: SizeScope,
    pub box_round: BoxRound,
    /// How many decimal places an app shows in a label by default (see [`crate::labels::format`]).
    pub label_places: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            kinds: KindTable::default(),
            choose: Choose::Nearest,
            places: Places::Compete { bias: 0.25 },
            matching: Matching::AnyToAny,
            frames: Frames { page: true, own: true },
            align_reach: None,
            spacing_reach: SpacingReach { times: 4.0, at_least: 0.0 },
            spacing_row: SpacingRow::Band,
            spacing_gaps: SpacingGaps::Row,
            size_axes: SizeAxes::SameAxis,
            angle_period: TAU,
            marks: MarkRule::Coincident,
            coincide: Coincide::default(),
            corners_keep_proportions: false,
            nudge: NudgeMode::Stop,
            size_scope: SizeScope::Anywhere,
            box_round: BoxRound::Corners,
            label_places: crate::labels::DEFAULT_PLACES,
        }
    }
}

impl Settings {
    /// Whether every number that is used is usable.
    pub(crate) fn is_usable(&self) -> bool {
        let r = self.spacing_reach;
        self.coincide.is_valid()
            && self.angle_period.is_finite()
            && self.angle_period > 0.0
            && r.times.is_finite()
            && r.times >= 0.0
            && r.at_least.is_finite()
            && r.at_least >= 0.0
            && match self.nudge {
                NudgeMode::Next { tick } => tick.is_finite() && tick > 0.0,
                _ => true,
            }
            && match self.size_scope {
                SizeScope::View(r) => r.x0.is_finite() && r.y0.is_finite() && r.x1.is_finite() && r.y1.is_finite(),
                SizeScope::Anywhere => true,
            }
            && match self.places {
                Places::Compete { bias } => bias.is_finite() && bias >= 0.0,
                Places::First => true,
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_documented_ones() {
        let s = Settings::default();
        assert_eq!(
            (s.choose, s.matching, s.marks, s.spacing_row, s.spacing_gaps, s.size_axes),
            (Choose::Nearest, Matching::AnyToAny, MarkRule::Coincident, SpacingRow::Band, SpacingGaps::Row, SizeAxes::SameAxis)
        );
        assert_eq!((s.frames, s.align_reach, s.angle_period), (Frames { page: true, own: true }, None, TAU));
        assert_eq!(s.places, Places::Compete { bias: 0.25 });
        assert_eq!(s.coincide, Coincide { abs: 1e-9, rel: 8.0 * f64::EPSILON, angle: 1e-9 });
        assert!(s.is_usable());
    }

    #[test]
    fn a_setting_that_is_not_a_number_is_not_usable() {
        let base = Settings::default();
        for bad in [
            Settings { angle_period: 0.0, ..base.clone() },
            Settings { angle_period: f64::NAN, ..base.clone() },
            Settings { coincide: Coincide { abs: f64::NAN, ..base.coincide }, ..base.clone() },
            Settings { spacing_reach: SpacingReach { times: -1.0, at_least: 0.0 }, ..base.clone() },
            Settings { places: Places::Compete { bias: f64::INFINITY }, ..base.clone() },
        ] {
            assert!(!bad.is_usable());
        }
    }
}
