#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

//! Snapping and smart guides for any editor that drags things on a canvas.

mod align;
pub mod budget;
mod check;
mod ctx;
pub mod frame;
pub mod grid;
mod index;
pub mod kind;
pub mod labels;
mod marks;
mod moves;
pub mod osnap;
pub mod query;
pub mod result;
pub mod settings;
mod solve;
pub mod targets;

pub use budget::{Budget, Cuts, Made, Refusal, Spend, Work};
pub use frame::{Anchors, Axis, Coincide, Direction, Id, OwnOrBounds, Side, TurnedBox};
pub use kind::{Kind, KindRule, KindTable, Reach};
pub use query::{About, Constraint, Detail, Edges, Lattice, NoDetail, Query, Rounding, Subject};
pub use result::{Broken, Hit, Mark, Snapped};
pub use settings::{Choose, Frames, MarkRule, Matching, Places, Settings, SizeAxes, SpacingGaps, SpacingReach, SpacingRow};
pub use solve::snap;
pub use targets::{BuildReport, Limits, Object, Parts, Refused, Targets, TargetsBuilder};
