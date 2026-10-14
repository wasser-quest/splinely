# Optional canvas rulers (egui 0.31)

`crates/forma_editor/src/rulers.rs` is a standalone module using the existing
`eframe::egui` dependency. It is deliberately **not wired into the app or library**.
The main integrator owns the enabled switch, `mod rulers;`, canvas layout, origin,
and input routing. No dependencies or manifests need to change.

## Exact public API

```rust
pub fn paint(painter: &egui::Painter, viewport: Rect, origin: Pos2, zoom: f32) -> RulerGeometry;
pub fn content_rect(viewport: Rect) -> Rect;
pub fn ruler_hit_test(viewport: Rect, pos: Pos2) -> Option<RulerHit>;
pub fn geometry(viewport: Rect) -> RulerGeometry;
pub fn ticks(viewport: Rect, origin: Pos2, zoom: f32) -> RulerTicks;
pub fn nice_interval(minimum: f64) -> Option<f64>;
```

`Rect` and `Pos2` are `eframe::egui` types. Returned types:

```rust
pub enum RulerHit { Corner, Top, Left }
pub struct RulerGeometry {
    pub content: Rect,
    pub top: Rect,
    pub left: Rect,
    pub corner: Rect,
}
pub struct Tick {
    pub screen: f32,
    pub document: f64,
    pub major: bool,
}
pub struct RulerTicks {
    pub horizontal: Vec<Tick>,
    pub vertical: Vec<Tick>,
}
```

Constants: `LEFT_GUTTER: f32 = 64.0`, `TOP_GUTTER: f32 = 24.0`,
`MAX_TICKS: usize = 512`, `MIN_ZOOM: f32 = 0.0001`,
`MAX_ZOOM: f32 = 10000.0`.

## Integration choices

Pass only the canvas viewport **after** toolbar/sidebar layout, not the entire
window. All ruler shapes and labels are clipped to this viewport, each ruler's
own rectangle, and the incoming painter clip. Paint rulers after canvas artwork
so the strips stay visible. This is passive painting: no widget IDs, responses,
selection, pan, zoom, history, or pointer capture are changed.

Gutter mode, schematic code in the existing canvas owner:

```rust
let canvas_rect = if rulers_enabled {
    rulers::content_rect(viewport)
} else {
    viewport
};
// Allocate/render the canvas in canvas_rect using the existing canvas transform.
// Derive origin from that SAME transform; it already includes gutter offset + pan.
// ... paint canvas ...
if rulers_enabled {
    rulers::paint(&painter, viewport, origin, zoom);
}
```

Overlay mode keeps the existing canvas rectangle and origin unchanged; simply
call `paint` after canvas drawing when enabled. The same 64-unit left/24-unit top
strips cover artwork instead of reserving layout space. In either mode, exclude
ruler hits from canvas gestures if desired:

```rust
let over_ruler = rulers_enabled
    && rulers::ruler_hit_test(viewport, pointer_pos).is_some();
```

Do not reserve gutters or reject hits when the switch is off. Hit testing uses
half-open viewport boundaries and assigns the intersection to `Corner`.
`content_rect` clamps to tiny viewports, possibly returning zero-sized content;
skip canvas interaction/rendering there as appropriate. Non-finite or inverted
viewports return zero-sized geometry at screen zero and no hits/ticks/painting.

## Coordinates and limits

The corner says **SVG u**: default px-like SVG document/user units, **not physical
millimeters**. No DPI calibration or physical-unit conversion is implied.
Screen positions obey exactly the canvas convention:

```text
screen.x = origin.x + document.x * zoom
screen.y = origin.y + document.y * zoom
```

`origin` is the screen position of document `(0, 0)`, including pan. It need not
be inside the viewport. Negative coordinates are supported and zero is labeled
`0`, never `-0`. Vertical labels are horizontal for readability.

Major intervals adapt through the 1/2/5 decade sequence with at least 60 logical
egui screen units between major positions before f32 screen rounding. Five minor
intervals subdivide each major interval. Wider viewports increase the interval
to keep the work bounded: at most 256 ticks per axis, **512 total**. Labels use
measured font bounds: labels not fully fitting their strip or overlapping an
already drawn label are omitted. Large/small numbers use compact scientific
notation. A dark neutral background distinguishes the rulers from canvas art.

Normal zoom 0.1–3 and the broader inclusive range 0.0001–10000 are supported.
Invalid/out-of-range zoom or non-finite origins yield empty tick vectors (the
ruler background/unit label may still paint). Zoom is rejected rather than
clamped, avoiding a ruler scale different from the canvas. Arithmetic uses f64;
indices beyond +/-2^52 are omitted rather than risking repeated ticks or an
unbounded loop. Only finite, strictly increasing screen positions are emitted;
very large coordinates can therefore yield fewer ticks or none. The source is
pure geometry plus a painter, with no GPU or document model dependency.

## Tests

Inline tests cover exact pan/zoom mappings, negative coordinates and positive
zero, nice intervals, gutter/hit boundaries, invalid inputs, tiny viewports,
huge finite coordinates, zoom endpoints, monotonic finite ticks and the combined
512-tick limit. Once the integrator declares the module, run:

```sh
TMPDIR=/home/user/svg_editor/.work-tmp cargo test -p forma_editor rulers::tests
```

Before integration, the same tests can run via `rustc --test` against an existing
eframe 0.31 build (no module registration or manifest edits):

```sh
# EFRAME_RLIB must point to the existing eframe 0.31 .rlib in target/debug/deps.
TMPDIR=/home/user/svg_editor/.work-tmp rustc --edition 2021 --test \
  crates/forma_editor/src/rulers.rs --extern eframe="$EFRAME_RLIB" \
  -L dependency=target/debug/deps -o .work-tmp/rulers-tests
.work-tmp/rulers-tests
```
