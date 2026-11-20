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
  `LikeToLike`, `SameSide`); the families of lines; spacing rows (`Band`; `Linked`, where a staircase of objects is a row; or `Beyond`, the rule of an app that reads a strip: the neighbour on each side and the nearest object wholly beyond it, and nothing else) and gaps; size axes; the angle period; how far a line mark runs
  (`LineSpan::Every`; `Nearest`, to the nearest target on one side and no farther; or `EachSide`, from the nearest target on one side of the subject to the nearest on the other, through the subject); and what counts as the same place (`Coincide`, which `Coincide::at(far)` widens by what a coordinate as large as
  `far` cannot say; a query does that itself for the largest coordinate of its subject and of the result).
- **Constraints and rounding are solved inside the snap.** An axis lock, a ray, a kept aspect ratio (from a corner or a side handle), sides that
  move together, a hard lattice of angles, and a rounding lattice all limit the candidates, so the result satisfies them. A candidate that a
  rounding takes off its line is dropped and the next is tried. A resize under a rounding rounds the point that leads the drag: the middle of a
  dragged side, or, with the proportions kept, the dragged corner. The leading point lands on the lattice along the page axis it moves along most,
  and the other coordinate follows the drag (or the ratio). This holds for a turned box as for one square to the page. A rounding that would
  collapse or flip the box is not made. The ratio of a kept aspect is the ratio the result has, not
  the dragged box's own; a nudge is not rounded.
- **Marks come from the final geometry.** After the geometry is known, each side of it is looked up again among the lines, so a mark is there if
  and only if the geometry lies on it: `Mark::Line`, `Gap` (with its value, in chains), `Size`, `Angle` and `Place`. `Snapped::check` verifies
  every mark, the constraint and the rounding by reading the targets by brute force; it is what the tests assert. It cannot read the detail
  source, so a place mark of an app's kind that names an object with detail is taken on trust there; `Snapped::check_with_detail` asks the
  source for the object's places and refuses a mark it does not give.
- **Equal spacing** snaps one gap and marks every equal gap in the row. **Size and rotation match**, **angle steps**, **object snaps** (vertex,
  midpoint, nearest, intersection, tangent, perpendicular on `BezPath`s), **lattices** and the zoom-following pitch helpers (`grid::doubling`,
  `grid::one_two_five`), and **label placement** (`labels::place`) are in the crate. The crate does not format numbers: an app formats each
  value in its own unit.
- **Nudge** goes along a page axis to the first target strictly ahead (a line of any kind, a lattice line, the next place that repeats a gap, and,
  with `Settings::nudge_places`, a place on the line one of its places travels along), within one step (`NudgeReach::Within`) or however far
  (`Anywhere`), else by a fallback distance. The nearest stop wins, whatever its kind and under either tie rule (`Choose::Ranked` too: a
  nearer place beats a better-ranked one farther on, and priority only breaks a tie between stops as near as each other). Its other delta is
  `+0.0` to the bit going either way, never `-0.0`: the two compare equal, but a comparison of the bits tells them apart.
- **Places on a line of travel.** A subject that travels along one page axis (a move held to it by `Constraint::Axis`, or a nudge) has a line
  of travel for each of its places: the places that are the same across the axis, by the same-place rule, travel along one. A target place
  (the app's own, the corners and side middles of other boxes, and what the detail source gives: connection points, vertices, midpoints,
  crossings of outlines) meets it only where it lies on one of those lines, and the subject lands on it, exactly, with the other delta 0 to the bit.
  `Settings::locked_places` turns this on for a held move: a place within the reach of its kind along the line, either way, pulls, and takes
  the degree before any line does (`Places` decides it as for any other move). `Settings::nudge_places` turns it on for a nudge: the first place
  strictly ahead on a line of travel within the nudge's own reach is a stop, and it is the stop if it comes before the line the nudge would
  stop on (a tie goes to the lower priority number, and to the line when those are equal). A line of travel is read as a strip of the grid, as
  long as the subject and the nudge's lookahead need: up to 2,304 buckets along when it lies in one bucket across, about 1,150 when it lies on a
  bucket boundary (a line at a whole number does, in buckets of 1); a longer one is cut round the subject's trailing place and says so, so a
  subject as wide as it likes has the places past each of its corners met, held or nudged. Both are `TravelPlaces::Ignore` by default, as the
  crate was before them: no place pulls a held move or stops a nudge. The subject's places are the nine of its box (corners, side middles,
  centre) and, for a held move, the `own` places that move with it; a place off every line of travel is not a candidate.
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
to the part round its middle, and says so (a strip read for a line of travel is the exception: see above). The number is a bound on the worst page, not a count of what a query does: an ordinary query gathers
fewer than a dozen windows, a query that is refused gathers none, and one that would gather more is cut at the sixteenth and reports it (`Cuts`, `truncated`).
Sixteen is kept on purpose: each re-check is one window of at most `window_visits` entries, so a query costs at most sixteen of those and no
more, however many candidates the page offers.

`Limits` (per target set): 200,000 objects, 100,000 of the app's own lines, 200,000 places, 10,000 paths, 2,000,000 path elements. A target past
a limit is refused and counted in `BuildReport`. Vectors grow with `try_reserve`, so a build that cannot get the memory refuses and says so.

Hostile numbers are refused or bounded: a subject, zoom, angle, step or ratio that is not finite gives no snap and a `Refusal`; a target that is
not finite is refused at the builder; a curve is charged for the pieces it would flatten into before it is flattened; a path is read about its
own local origin, so a small outline far from the origin snaps as it does at the origin; the bucket grid takes its origin from the targets and
bounds its bucket numbers to ±2^31.

Two settings are fixed for a target set once a query has used them: the sorted angles are made for the first `angle_period` asked for (a query
with another gets no rotation match, and its work says `truncated` with the `ANGLE_PERIOD` cut), and lines are grouped by the `Coincide` the builder was given in `Limits`.

## What it does not do

- **A resize has no equal-gap candidates.** `Subject::Edges` snaps to lines, places and sizes, not to a gap that repeats a neighbour's; a move does
  both.
- **Places pull a held move only on a page axis, and only when asked.** `Settings::locked_places` makes `Constraint::Axis` meet the places on the
  lines its places travel along; `Constraint::Ray` solves against lines only. A place that is near a line of travel but not on it (past the
  same-place rule) is not a candidate, because the subject could not land on it: nothing is drawn that the subject does not land on.
- **A nudge's places are the nine of its box.** `Subject::Nudge` carries no `own` places, so connection points that move with the shape are not
  met by it; and a place is looked for as far as 46 buckets of the finest grid past the subject's leading place, or only up to the line it would
  stop on. A nudge that looks as far as that and finds the grid holding something past it says so with the `WINDOW_VISITS` cut (it cannot tell
  what is there without reading it, and a place off every line of travel counts); when the grid holds nothing past it, nothing is said.
  `CANDIDATES` is set when a strip held more than 64 places or objects (the 64 nearest the trailing place are kept) or was longer than a strip may
  be. `Within(step)` reads the same-place rule past the step too, since a place that far past it is a stop. A lattice no coarser than the
  same-place rule has no stops for a nudge (its lines are one place with the side, or with each other).
- **A wide subject turned off the page axes shares its strips.** A box turned by other than a quarter turn has nine lines of travel, joined into at
  most four strips by how near they lie; a strip that has to cover more than 2,304 buckets in all (a turned box a hundred buckets across) is cut
  round its trailing place and says so. A box square to the page, or a quarter turn from it, has three lines and none of this.
- **A scope that is not `All` is read by `Kind::Size` only.** `KindRule::scope` on any other kind looks at every object.
- **Only the nearest line of each kind and side is a candidate.** If a verification (a resize that would leave no box, a row that changed) drops it,
  the next nearest line of the same kind is not tried. The result always satisfies `Snapped::check`; it is not always the best one there was.
  Equal gaps are the exception: a direction that keeps an equal-gap candidate keeps up to seven more behind it, since a gap is read again in the
  row it lands in and may no longer repeat there.
- **Row neighbours are read from a strip.** An equal gap is searched among the objects that meet the strip `spacing_reach` long along the row, and
  only the objects that overlap the subject across it by more than the same-place rule (or, linked, are chained to those that do; or, `Beyond`, the
  neighbour on each side and the nearest object wholly beyond it that overlaps it across). `Beyond` does not step onto an object that overlaps the
  neighbour along the row, and an object that touches the neighbour is no gap to repeat (a gap of nothing is not drawn).
- **Far from the origin a place is coarser.** The same-place rule grows by a few units in the last place of the largest coordinate in the query, so a
  result is not called off its line for the rounding of its own coordinates; two candidates that are closer than that at 1e11 are told apart by
  the rule, not by their distance.
- **Lines are grouped once, when the targets are built.** Two lines the same-place rule calls one place are one entry with a count, by the rule in
  `Limits::coincide`; the query widens its own rule far from the origin (`Coincide::at`), but the entries were made before. Two edges that are one
  place near the origin can be two entries at 1e11 (a mark then counts one target where it counted two). A set that lives far from the origin should be
  built with `Limits { coincide: Coincide::default().at(far), .. }`, which is what the translation property does to compare what the engine can promise.
- **A resize far from the origin of a box tilted a hair off the page can lose a mark.** At about 1e6 from the origin, a box tilted 1e-8 to 1e-9
  radians (inside the tolerance that calls it square to the page, but more than the numbers of its own axes there can hold) resized by a handle can
  return a hit on a grid line with no mark for it. The mark is left out; a mark is never drawn that is not true. Known, not fixed.
- **A set is indexed about one origin.** It is the median of the object centres, so one cluster of objects at 1e11 snaps as one at 0, but two
  clusters farther apart than 1e11 cannot both be exact: the smaller one is as coarse as the numbers at the larger. A query whose subject is
  farther than 1e11 from that origin (with anything in the set) says `truncated` with the `FAR_CLUSTER` cut, and a query among the cluster the origin
  was taken from does not.

## Measured costs

*Measured* (counted, in the units the loops walk; `cargo test -p craft-snap --test it measure -- --ignored --nocapture`). No test asserts time.

| Query | Objects | Lines | Bisection steps | Entries walked | Windows gathered |
|---|---|---|---|---|---|
| Alignment only, anywhere on the page | 100 | 1,200 | 264 | 1 | 0 |
| | 10,000 | 120,000 | 372 | 2 | 0 |
| | 100,000 | 1,200,000 | 438 | 2 | 0 |
| | 100,000 at 100,000 different angles | 1,200,000 | 978 | 2 | 0 |
| All kinds on, 5,000 objects | 5,000 | 60,000 | 366 | 2 | 9 (281 entries visited) |

Places on a line of travel (`Settings::locked_places`, `nudge_places`; *measured*, the extra work over the same query with places left out):

| Input | Bucket entries visited | Places weighed | Objects kept |
|---|---|---|---|
| a held move, 40 places on its line | 80 | 40 | 40 |
| a held move, 100,000 places on its line | 8,192 (two strips at the cap of 4,096) | 64 | 64 |
| a nudge, 100,000 places on its line | 8,192 | 64 | 64 |
| 100,000 places a hair off the line, beyond twice the same-place rule | 4,096 | 0 | 0 |
| 100,000 places between once and twice the rule, or within it | 8,192 | 64 | 64 |
| a held move, 100,000 boxes whose corners and middles are on its lines | 12,288 (three strips) | 512 | 192 (of 100,000), 16 outlines asked |

A line of travel is one strip of each grid (at most four strips, so a turned box's nine lines and the lines of what moves with it are joined by
how near they lie), and each strip is one window of the 16 a query has. The same scene at 1e6, 1e9 and 1e11 gives the same hits and the same
counts. Timed beside forty places in a release build (`--ignored timed`): a held move among 100,000 about 25 times, a nudge about 3 times.

A line mark under `LineSpan::EachSide` over a hundred thousand coincident members costs five bisections of 17 steps more than `Every` does (89 bisection
steps against 4, *measured*, the same one entry walked): the nearest member on each side is found by bisection, not by a walk over the members.

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
- **EffectCraft's** place pulled as a line across a locked drag is not ported as a line: a place pulls a held move only where the subject can land
  on it, on the line one of its places travels along (`Settings::locked_places`), and a ray still meets lines only. Its bias of a place against a
  pair of lines is `Places::Compete`.
- **PhotoCraft** applies its Shift lock after the snap; here a constraint is solved inside it.

## Tests

- `Snapped::check` is asserted by property tests over random scenes, settings, zooms, constraints and roundings, for every subject, turned and
  not. Five planted mutants fail it (marks before the constraint, marks before the rounding, a lock after the snap, a mark for a target that is
  not there, a turned resize rebuilt along the page axes). A sixth, candidates of a turned resize solved with the wrong travel, passes it, because
  a candidate that does not hold on the final extents is dropped, so it only loses snaps; the named case of a turned box dragged by a side onto a
  guide fails it.
- Counted cost tests for each hostile input: 100,000 objects (also at 100,000 angles), a stack of 2,500 boxes, a row of 10,000 equal gaps, far
  coordinates to 1e11, nearly parallel directions, 100,000 lines at one place, 100,000 distinct lines in one reach, 100,000 equal widths, one match
  in view among 100,000 outside it, a nudge with nothing ahead and one whose next target is 1e9 away, hostile angles and steps, degenerate boxes
  and rows (no width, touching, overlapping, a box wider than its room, a resize that would leave no box), every shortcut checked against the
  inputs it must still catch, and every budget field at nothing and at one. The numbers are pinned near the measured work. The `#[ignore]`d
  `timed` tests time each case beside a yardstick, back to back (`cargo test -p craft-snap --test it --release -- --ignored timed`).
- Two properties over random scenes: a scene turned as a whole gives the same hits and marks and the move turned by the same angle, and a scene
  moved to 1e3, 1e6, 1e9 and 1e11 gives the same hits and marks (to 1e9; at 1e11 the result is checked, for candidates closer than the numbers there
  can tell apart are told apart by the noise). The seed is fixed; `CRAFT_SNAP_CASES` and `CRAFT_SNAP_SEED` run a longer search, and a find is a
  case to read.
- Places on a line of travel, against a scan of every place and every place of the subject: the held move goes to the nearest place within reach,
  the nudge to the nearest strictly ahead, on a lattice where places meet lines by themselves and for boxes turned by 0, a quarter and 0.5 radians
  (1,500 cases each), and the same scene mirrored, moved to 1e3, 1e6 and 1e9, and turned by a quarter gives the same result. Counted costs for
  100,000 places on the line, a hair off it and far from the origin. The invariant (`Snapped::check`) runs with both settings on.
- A line mark from the nearest target on each side (`LineSpan::EachSide`): its spans against a scan of every span, mirrored and moved, a hundred
  thousand members at one place (five bisections an entry), and the check shown to fire on a segment that stops short, runs on, or reaches one
  side only.
- Order, mirror, quarter-turn, angle-0 and zoom-boundary checks, a scene moved to 1e6, 1e9 and 1e11 (the same hits and marks), and the index against a scan of every line (anywhere on the page and within a reach across).
