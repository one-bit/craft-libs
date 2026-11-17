#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

//! Snapping and smart guides for any editor that drags things on a canvas.

pub mod budget;
pub mod frame;
mod index;
pub mod kind;
pub mod osnap;
pub mod targets;

pub use budget::{Budget, Cuts, Made, Refusal, Spend, Work};
pub use frame::{Anchors, Axis, Coincide, Direction, Id, OwnOrBounds, Side, TurnedBox};
pub use kind::{Kind, KindRule, KindTable, Reach};
pub use targets::{BuildReport, Limits, Object, Parts, Refused, Targets, TargetsBuilder};
