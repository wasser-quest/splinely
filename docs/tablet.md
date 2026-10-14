# Freehand and tablet input

`crates/forma_editor/src/freehand.rs` is a reusable, staged canvas tool. It is
not wired into main yet. No dependency, manifest, history, or toolbar changes are
required by this module. Its egui APIs work with 0.33 (also tested with the
repository's current 0.31 dependency).

## Integration contract

Add `mod freehand;` in the module owning the canvas and store
`freehand::FreehandTool::default()` in editor state. The exact integration API is:

```rust
pub fn handle(
    &mut self,
    ui: &egui::Ui,
    painter: &egui::Painter,
    response: &egui::Response,
    origin: egui::Pos2,
    zoom: f64,
    project: &forma_document::Project,
    enabled: bool,
) -> Result<Option<(forma_document::Project, String)>, String>;

pub fn cancel(&mut self);
pub fn is_drawing(&self) -> bool;
pub fn samples(&self) -> &[Sample];

pub fn stroke_candidate(
    project: &forma_document::Project,
    id: &str,
    samples: &[Sample],
    options: FreehandOptions,
) -> Result<forma_document::Project, String>;
```

Call `handle` **once per canvas frame**, after drawing the existing scene, using
an enabled canvas response allocated with `egui::Sense::drag()`. `origin` is the
screen position of document `(0, 0)`, **including pan**. Mapping is
`document = (screen - origin) / zoom`; pass the caller's zoom as `f64`. Created
nodes have identity transforms and document-space coordinates, not coordinates
local to an existing selected node.

Resolve pan ownership first. Pass `enabled = false` for panning, another active
tool, playback/animated preview, modal UI, or an inactive canvas. Do not merely
skip the call on those frames: disabled frames erase the draft, so a later
release cannot create a ghost stroke. Call `cancel()` explicitly when switching
away from a canvas which will no longer receive frames. The tool does not consume
egui events: while Freehand owns the canvas, do not also run selection, node
dragging, Pen, or handle-edit gestures on the same events. `is_drawing()` alone
is not an ownership flag, since it becomes false on release; use the caller's
selected-tool state.

```rust
match freehand.handle(ui, painter, response, origin, zoom as f64,
                      &project, freehand_selected && !pan_active && edit_enabled) {
    Ok(Some((next, id))) => {
        // Use the editor's existing single history transaction:
        commit(next);
        selected = Some(id);
    }
    Ok(None) => {} // drawing, cancellation, or no gesture
    Err(error) => { status = error; }
}
```

The returned ID is unique across the project namespace (`freehand_1`, etc.).
Selection and history are caller-owned. A stroke returns exactly one validated
candidate on release, never a live mutation. Commit it immediately, once. A tap
without movement produces no node. A press outside the canvas cannot become a
stroke by dragging inside; egui hit testing also excludes occluding widgets.
Release outside the canvas completes an already-owned stroke. Escape,
`PointerGone`, focus loss, touch cancellation, disabled frames, and errors erase
the draft. Changing the project or viewport mid-gesture cancels with an error;
options are snapshotted at press time.

## Geometry and limits

`FreehandOptions` exposes `width`, `tolerance`, `pressure_outline`, `outline`,
and `color`. Defaults: width **3 document units**, simplification tolerance
**0.5 document units**, pressure outlines enabled, forced outlines disabled,
and dark gray. Tolerance is not a screen-pixel value and is independent of zoom.

- `Sample` stores screen position, document position, normalized pressure, and
  whether that pressure was measured.
- At most **4096 distinct consecutive positions** are retained. Overflow cancels
  the entire gesture with an error, rather than truncating and committing it.
- Iterative bounded Ramer–Douglas–Peucker simplification preserves endpoints and
  centerline shape within the configured tolerance; variable-pressure width
  changes also participate in simplification. Worst-case work is quadratic in
  the bounded sample count; no recursive stack growth.
- Without measured pressure the node is a constant-width, no-fill SVG path.
  `outline = true` instead requests a constant-width filled SVG outline.
- With measured pressure and `pressure_outline = true`, a filled, closed
  nonzero outline encodes varying width. Same-winding overlapping segment pieces
  and round joints/caps keep retraces and crossings filled, without unbounded
  miters. Minimum pressure width is 5% of the configured width. This is simple
  outline geometry, not a calibrated brush; preview segments approximate it.
- Both forms are ordinary editable `NodeKind::Path` nodes with recalculated
  `path_bounds`, not baked raster images. Outline editing manipulates the
  boundary rather than a stored centerline/pressure curve.
- Invalid/nonpositive/nonfinite zoom, invalid origin, nonfinite samples/options,
  coordinates outside +/-10,000,000, malformed geometry, duplicate/invalid IDs,
  or invalid project candidates are rejected. The input project is never
  partially changed.

## Stylus on Linux: what this does and does not provide

On X11 and Wayland this tool consumes whatever input the existing egui backend
actually delivers. It is **not a Wacom driver**, does not configure libinput,
XInput, compositor tablet protocols, device calibration, or tablet mapping,
and does not guarantee native stylus pressure on either display server.

Pressure is read only from `egui::Event::Touch { force: Some(..), .. }` when
supplied by the backend. Finite values are clamped to `[0, 1]`; missing or
nonfinite force falls back to full-width pointer pressure. A missing-force
stationary touch-up does not erase the last measured pressure. Ordinary
`PointerButton`/`PointerMoved` events have no pressure and use constant-width
fallback. A Linux tablet may appear only as pointer events, even if its native
driver knows its pressure. Backend/device/compositor support must be verified
on real hardware rather than inferred from these synthetic tests.

Touch events take precedence over synthetic pointer events in the same frame,
preventing duplicate strokes. The first eligible contact is tracked by device
and touch ID; other contacts are ignored while it draws. This is **not robust
palm rejection**: a palm can be the first contact, and egui touch input here does
not identify pen vs finger. Tilt, rotation, proximity, eraser tools, stylus side
buttons, tablet-specific gestures, and native driver-level palm rejection are
not implemented. Disable drawing during caller-owned pan/multitouch gestures.

## Standalone verification

The tests live in the same module; no changes to main/lib or manifests are
needed to include them later. Until the integration declares the module, normal
workspace tests will not discover them. A separate temporary harness can
compile them directly against egui 0.33:

```sh
# Run from the repository root. Keep tempfiles/builds off a full /tmp.
export TMPDIR="$PWD/.work-tmp"
harness="$TMPDIR/freehand-test"
mkdir -p "$harness/src"
printf '[package]\nname="forma_freehand_test"\nversion="0.1.0"\nedition="2021"\n[workspace]\n[dependencies]\negui="0.33"\nforma_document={path="%s/crates/forma_document"}\n' "$PWD" > "$harness/Cargo.toml"
printf 'extern crate self as eframe;\npub use egui;\n#[path="%s/crates/forma_editor/src/freehand.rs"]\npub mod freehand;\n' "$PWD" > "$harness/src/lib.rs"
cargo test --manifest-path "$harness/Cargo.toml"
```

The harness aliases egui's re-export only; this module uses no eframe-specific
functionality. Nine tests cover straight/curved/noisy simplification and its
error bound, pressure/fallback/outline geometry, pan/zoom transforms, invalid
viewports/resources/IDs/projects, atomic release, outside starts, cancellation,
touch deduplication and contact identity, stationary force retention, focus
loss, and public-handle disabled/Escape/resource-error behavior.
