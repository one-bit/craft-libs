# craft-snap

Snapping and smart guides for any editor that drags things on a canvas. `craft-snap` decides where a dragged point, box, edge or angle comes to
rest, and what to draw about it. It depends on `kurbo` and nothing else at run time (no egui, no threads, no clock), works in `f64` and runs on
`wasm32`.

## What it does

- **One target set per gesture.** `TargetsBuilder` takes objects (a `TurnedBox` each, with the box it aligns by on the page axes when that is not
  the box round its corners), lines in any direction, places, and outlines; `build()` only stores and sorts. Each index is made the first time a
  query needs it, and the work report says which. The set does not depend on the order things were added in.
- **One query** for move (`Subject::Box`), point (`Subject::Point`), resize and draw (`Subject::Edges`), rotate (`Subject::Rotate`) and nudge
  (`Subject::Nudge`): `snap(&targets, &query, &mut detail) -> Snapped`.
- **Turned shapes are first-class.** Targets and the subject carry their own turned frames. Alignment, resize, equal gaps and size match work
  along each shape's own axes as well as the page's. A shape a whole number of quarter turns from the page is the page's own case, stored once.
- **Policy is data.** `Settings` holds, per kind, whether it is on, how far it pulls (`Reach::Screen`, `Document` or `Any`), its priority and
  its scope; the tie rule (`Choose::Nearest` or `Ranked`); how places compete with lines; which sides meet which lines (`Matching::AnyToAny`,
  `LikeToLike`, `SameSide`); the families of lines; spacing rows and gaps; size axes; the angle period; how far a line mark runs
  (`LineSpan::Every` or `Nearest`); and what counts as the same place (`Coincide`).
- **Constraints and rounding are solved inside the snap.** An axis lock, a ray, a kept aspect ratio (from a corner or a side handle), sides that
  move together, a hard lattice of angles, and a rounding lattice all limit the candidates, so the result satisfies them. A candidate that a
  rounding takes off its line is dropped and the next is tried.
- **Marks come from the final geometry.** After the geometry is known, each side of it is looked up again among the lines, so a mark is there if
  and only if the geometry lies on it: `Mark::Line`, `Gap` (with its value, in chains), `Size`, `Angle` and `Place`. `Snapped::check` verifies
  every mark, the constraint and the rounding by reading the targets by brute force; it is what the tests assert.
- **Equal spacing** snaps one gap and marks every equal gap in the row. **Size and rotation match**, **angle steps**, **object snaps** (vertex,
  midpoint, nearest, intersection, tangent, perpendicular on `BezPath`s), **lattices** and the zoom-following pitch helpers (`grid::doubling`,
  `grid::one_two_five`), and **label placement** (`labels::place`) are in the crate. The crate does not format numbers: an app formats each
  value in its own unit.
- **Nudge** goes along a page axis to the first target strictly ahead (a line of any kind, a lattice line, or the next place that repeats a gap),
  within one step (`NudgeReach::Within`) or however far (`Anywhere`), else by a fallback distance.
- **Bounded work, reported.** Every cap is charged in real units before the work it stands for; `Snapped::work` holds the counts, the caps met
  (`cut`), the indexes made, and whether anything was left out (`truncated`).

## Limits

`Budget` (per query; each is charged before the work):

| Cap | Counts | Default |
|---|---|---|
| `window_visits` | bucket entries visited in one window | 4,096 |
| `candidates` | objects kept near the subject | 64 |
| `own` | the subject's own places pulled with a box | 32 |
| `line_steps` | sorted-array entries walked past a bisection | 4,096 |
| `solves` | combinations solved for a move, and fallbacks | 512 |
| `points` | places weighed | 4,096 |
| `outlines` | objects whose outline is asked for | 16 |
| `pieces` | path segments read, curve pieces estimated, pieces crossed | 16,384 |
| `pairs` | pairs of outlines crossed | 64 |
| `row` | objects in equal-gap rows | 256 |
| `marks` | marks emitted | 64 |
| `detail` | the detail source's own units | the app's |

A query gathers at most `MAX_GATHERS` (16) windows from the bucket grids: up to five to find its candidates (round the subject, and one row along
each of up to four directions) and the rest to check a candidate again on the geometry it would land on. A window wider than 48 buckets is cut
to the part round its middle, and says so.

`Limits` (per target set): 200,000 objects, 100,000 of the app's own lines, 200,000 places, 10,000 paths, 2,000,000 path elements. A target past
a limit is refused and counted in `BuildReport`. Vectors grow with `try_reserve`, so a build that cannot get the memory refuses and says so.

Hostile numbers are refused or bounded: a subject, zoom, angle, step or ratio that is not finite gives no snap and a `Refusal`; a target that is
not finite is refused at the builder; a curve is charged for the pieces it would flatten into before it is flattened; a path is read about its
own local origin, so a small outline far from the origin snaps as it does at the origin; the bucket grid takes its origin from the targets and
bounds its bucket numbers to ±2^31.

Two settings are fixed for a target set once a query has used them: the sorted angles are made for the first `angle_period` asked for (a query
with another gets no rotation match and says so), and lines are grouped by the `Coincide` the builder was given in `Limits`.

## Measured costs

*Measured* (counted, in the units the loops walk; `cargo test -p craft-snap --test it measure -- --ignored --nocapture`). No test asserts time.

| Query | Objects | Lines | Bisection steps | Entries walked | Windows gathered |
|---|---|---|---|---|---|
| Alignment only, anywhere on the page | 100 | 1,200 | 264 | 1 | 0 |
| | 10,000 | 120,000 | 372 | 2 | 0 |
| | 100,000 | 1,200,000 | 438 | 2 | 0 |
| | 100,000 at 100,000 different angles | 1,200,000 | 978 | 2 | 0 |
| All kinds on, 5,000 objects | 5,000 | 60,000 | 366 | 2 | 9 (281 entries visited) |

A query a hundred thousand objects far from the pointer costs the same bisections as one beside it. One object in view among 100,000 of the same
width outside it is found by one bisection, after a counted scan of 100,001 bounds when the `ViewIndex` is made.

Memory (*measured*, `size_of`): 112 bytes an object, 56 bytes a line while the lines are sorted, 56 bytes a place of a line, 32 bytes a member of
a group of coincident lines, 40 bytes an app's place. A build at the default limits with every object turned runs in a child process under a
3 GB limit in the tests and refuses the targets past them.

## Where the code came from

Each ported file says where it came from on its first lines. All sources are MIT OR Apache-2.0, as this repository is.

| Crate code | Source | Commit | What came over |
|---|---|---|---|
| `osnap` | CADCraft `crates/engine/src/snap.rs`, `crates/engine/src/snap/curves.rs` | `ba3196e3f737f2fed0d4d8ae9c065b20773dec16` | Endpoint, midpoint by arc length, intersection near the aperture, nearest, tangent and perpendicular by sampled roots, the 15% score. Moved onto kurbo `BezPath`, read about a local origin, and bounded |
| `labels` | FigCraft `crates/ui-egui/src/canvas/measure_overlay.rs` (`place_labels`) | `4548d6c00953dd051ee36af3bada376d1a80d05c` | The order of label spots. Moved from egui types to kurbo, and bounded |
| `spacing` (the three equal-gap cases) | FigCraft `crates/engine/src/canvas/snap.rs` (`spacings`) | the same | Centred between neighbours, and the gap repeated on either side |
| `spacing` (gap pairs and chains) | VectorCraft `crates/tools/src/guides/targets.rs` (`even_spacing`, `gap_marks`) | `9a01d266b0438f96608c9f75b51437f912587d22` | Each object paired with the next by bisection, any gap in the row repeated, every equal pair marked |
| `index` (sorted lines) | VectorCraft, the same file | the same | Sorted arrays searched by bisection, and the test that bisection equals a scan |
| `resize` (resize in a turned frame) | VectorCraft, the same file (`snap_scale`) | the same | One parameter per dragged side or proportional corner; the handle and its side's ends as the points that meet lines; collapse and flip refused |
| `resize`, `angle` | DesignCraft `crates/tools/src/snap.rs` (`pass_dimensions`, `length_edge_delta`, `match_rotation`) | `47ef1ec411d53e98a31c576d80a1d0e20ef5eb04` | Other objects' own side lengths; the length difference applied at the moving side; rotation match within `atan(zone / radius)` |
| `moves` (ties, bias) | EffectCraft `crates/engine/src/viewer.rs` (`better`, `POINT_BIAS`) | `60e2cbbbe3d73c0a9074c25a1a615ccfec18721c` | Priority only on ties; the place's bias against a pair of lines |
| `targets` (shape of the set) | PhotoCraft `crates/engine/src/snap.rs` (`SnapTargets`) | `957b10cccbb4bcd402394d250e52a9b9e1b63d74` | A plain target set built at the press, a span on every target, every side exactly on a target reported |
| `index` (grid and its scan test) | PDFCraft `crates/measure/src/snap.rs` | `efd265563edfe1b5a3261b184fbc4c83f67f4364` | A uniform grid with a list of wide entries, tested equal to a full scan |

Written for this crate: the bucket grid visited from the middle outward; the budgets and their report; the kind table and its two choosing rules;
the directions and the pairing of up to four of them; the constraint solver; the marks pass and its check; the view index; the nudge.

## Differences from the sources

- **CADCraft's object snaps** work on its own `Segment`, `Ellipse` and `Spline`; here every outline is a `BezPath`. Quadrant, centre, node,
  insertion, extension and parallel are not ported (they need an object kind a path has lost), nor are its tracking and its deferred snaps. A path
  is read about the centre of its control box, its size (not its distance) is bounded, and the flatness is the caller's.
- **FigCraft's label placement** is `f32` and egui-typed; here it is `f64` on kurbo types, capped at 32 labels, 96 spots and 64 boxes to avoid.
  FigCraft's equal spacing moves only and marks two gaps; here the gap repeated may be any in the row, every equal gap is marked and joined into
  chains, and the row is gathered from an index under a cap.
- **VectorCraft's `even_spacing`** is linear over up to 20,000 boxes; here a row is capped at 256 objects, gathered nearest first. Its lines are
  sorted per axis; here they are sorted per direction (any angle), per kind, with equal places grouped. Its `snap_scale` searches one parameter
  for a side or a proportional corner in an oriented box; here the same parameters are degrees of a general solve (a side, a half-size, a scale
  about a fixed point), a size match is a degree's candidate too, and a candidate that does not hold on the final extents is dropped.
- **DesignCraft** passes in order and the first hit wins; here the settings choose. Its size match reads every item; here the lengths are in
  sorted arrays (or in a `ViewIndex`). Its rotation tolerance is the same, `atan(zone / radius)`.
- **EffectCraft's** place pulled as a line across a locked drag is not ported: under an axis lock or a ray only lines pull. Its bias of a place
  against a pair of lines is `Places::Compete`.
- **PhotoCraft** applies its Shift lock after the snap; here a constraint is solved inside it.

## Tests

- `Snapped::check` is asserted by property tests over random scenes, settings, zooms, constraints and roundings, for every subject, turned and
  not; the planted mutants that must fail it (marks before the constraint, marks before the rounding, a lock after the snap, a mark for a target
  that is not there, a resize along the page axis) do.
- Counted cost tests for each hostile input: 100,000 objects (also at 100,000 angles), a stack of 2,500 boxes, a row of 10,000 equal gaps, far
  coordinates to 1e11, nearly parallel directions, 100,000 lines at one place, one match in view among 100,000 outside it, a nudge with nothing
  ahead and one whose next target is 1e9 away, hostile angles and steps, and every counter against its cap.
- Order, mirror, quarter-turn, angle-0, zoom-boundary, translation and index-against-scan properties.
