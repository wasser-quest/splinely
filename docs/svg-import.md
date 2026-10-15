# SVG import

`splinely-cli import-svg` and the editor's **Datei → SVG importieren** turn an SVG into editable
nodes. The import is **all-or-nothing for unsupported appearance** (clipping, masks, filters,
patterns, dashes, external files … cancel the whole import with a diagnostic) and
**always reports** what it imported, approximated or dropped. Nothing is dropped silently.
It is not a general SVG renderer: see [Not supported](#not-supported).

## Report

Every import produces a structured report (`ImportReport` in `splinely_document`):

| Field | Meaning |
|---|---|
| `imported`, `approximated`, `dropped`, `rejected` | exact totals |
| `diagnostics[]` | `status` (`imported` \| `approximated` \| `dropped` \| `rejected`), `code`, `element`, `id`, `attribute`, `message`, `count` |
| `truncated` | the list was capped at 5000 entries (totals stay exact); repeated losses are merged into one entry with `count` |

* **imported** – became a node/group (one entry per node, `code: "node"`).
* **approximated** – kept, but not exactly (e.g. `text-anchor: middle`, stroke gradient, gradient geometry).
* **dropped** – has no equivalent and was left out (metadata, editor-only attributes, raster images, `font-family`…).
* **rejected** – the import was cancelled; nothing was created.

CLI:

```sh
splinely-cli import-svg drawing.svg --output drawing.sply            # JSON result contains "report"
splinely-cli import-svg drawing.svg --output drawing.sply --strict   # fail (nothing written) if anything is approximated/dropped
splinely-cli import-svg drawing.svg --output - --report report.json  # project on stdout, report to a file (`-` = stderr)
```

A refused import exits with `error.code: "unsupported_svg"` and the same report as `error.importReport`
(`--strict` uses `svg_import_lossy`). The editor opens an **import report** window after every import
(also after a refusal; the project then stays unchanged). The per-entry `message` is English
technical text from the document crate; everything around it is translated.

## Supported

* Shapes `path rect circle ellipse line polyline polygon`, `g` (id → group, nested), `defs`, `symbol`,
  `use` (same-document `#id` only), solid fill/stroke, opacity, `fill-rule`, `transform` (baked into the path).
* **Text** `<text>`/`<tspan>` → `text` nodes (`x`, `y`, `font-size`, fill, opacity; node transform keeps rotation and scale).
  *Choice:* one node per `<text>`; tspans are concatenated (XML whitespace collapsed). A tspan with its own
  `x`/`y`/`dx`/`dy` starts a **new node** (`id`, `id-2`, …) because the model has one position per text node.
* **Gradients** `linearGradient`/`radialGradient` with stops, `gradientUnits` (`objectBoundingBox`/`userSpaceOnUse`),
  `gradientTransform`, `spreadMethod` (pad only), `href` inheritance (stops and attributes, chain ≤ 64, cycles rejected),
  referenced as `fill="url(#id)"` or in `style`. Mapped onto the model's bounding-box gradient (angle + ≤ 16 stops).
* **Images** with `data:` URIs: `image/svg+xml` (base64 or percent-encoded) is imported as a group of vector nodes
  (placement incl. `preserveAspectRatio`). `image/png` / `image/jpeg` are validated (signature, dimensions, CRC/markers)
  and then **dropped with a diagnostic** – see below.
* Inkscape/Figma-style files: `sodipodi:`/`inkscape:` attributes and defs-level editor elements are reported as dropped.

## Approximated

* Text width is **estimated** (0.6 × font size per character, same heuristic as the runtime) – used for `text-anchor: middle|end`
  (x is shifted) and the editor's selection box. Per-glyph `x/y/dx/dy/rotate` lists use the first value, tspan style
  differences are merged into the text element's style, skew in a text transform is dropped.
* Stroke gradients: the model has no stroke gradient → first stop colour.
* Gradient geometry: the model's linear gradient is a fixed vector of length 1 through the box centre; other start/end points are
  mapped onto it (stops outside are re-interpolated, `gradient-range`), radial gradients become centred with radius ½
  (centre/radius/focal offsets reported), `spreadMethod: reflect|repeat` → pad, > 16 stops are resampled, degenerate gradients → solid colour, skewed gradients flagged.
* Nested SVG images are not clipped to their box (`image-clip`).

## Not supported

* **Raster images (png/jpeg) are not imported.** The document has no image node kind and no image asset store
  (only audio assets), so embedding them would be faked. They are checked (≤ 16384 px per edge, ≤ 100 Mpx, ≤ 3 MiB each,
  8 MiB total, ≤ 32 images) and reported as `image-raster` with format, size and placement. GIF/WebP and other types are rejected.
* **External resources never load.** `http(s):`, `file:` and relative image URLs, `<use href="file.svg#x">`, external gradient
  references and `@import` are rejected (`image-external` / `reference-external` / `url-unsupported`); nothing is fetched or read.
* Fonts: `font-family`, weight, style, letter-spacing, decoration, `textPath`, vertical text are not stored (model has `font-size` only).
* Clip paths, masks, filters, patterns, markers, blend modes, group opacity over several shapes, stylesheets (`<style>`), dashes,
  non-default caps/joins, `vector-effect`, stroke-before-fill, scripts, event attributes, `foreignObject`, animation elements.

## Limits (hostile input)

| Limit | Value | Reject code |
|---|---|---|
| source size | 4 MiB | `limit-source` |
| XML nodes / elements / depth | 30 000 / 10 000 / 128 | `svg-malformed` / `limit-elements` / `limit-depth` |
| `<use>` expansion (billion laughs) / chain depth | 50 000 elements / 256 | `limit-expansion` / `limit-use-depth`, cycles `use-cycle` |
| gradients / stops per gradient / stops total / href chain | 512 / 256 / 4096 / 64 | `limit-gradients` / `limit-stops` / `limit-gradient-chain` |
| text chars / segments | 65 536 / 256 | `text-limit` |
| images / bytes each / bytes total | 32 / 3 MiB / 8 MiB | `limit-images` / `image-too-large` |
| nested SVG images | 8, each ≤ 512 KiB, depth 1 | `limit-nested-svgs` |
| imported nodes | 20 000 | `limit-nodes` |
| DTD / entities | not allowed | `svg-malformed` |

The expansion size of `<use>` trees is computed (memoised) *before* usvg sees the file, so exponential chains are refused cheaply.

## Source text vs. visual editing (conflict indicator)

The app's "source text" is `Project.rawSvg` (editor tab *Erweitert → SVG-Quelltext*): it is preserved and XML-checked but never
re-imported, and visual edits never rewrite it. After an import both sides therefore can drift apart silently. `import-svg` now stores
a baseline in the optional project field `svgImport` (`sourceHash`, `documentHash`, both `fnv1a64:<16 hex>`; the document hash covers
size, groups and nodes, **not** animation). `Project::source_sync()` compares it:

| State (`inspect` → `svgSourceSync`) | Meaning |
|---|---|
| `inSync` | unchanged since import |
| `visualEdited` | nodes changed → the stored source text is stale |
| `sourceEdited` | source text changed → the drawing does not show it |
| `both` | both changed |
| `noBaseline` | not imported from SVG (or older project) |

The editor shows a warning in the status bar and the SVG-source tab, with a button to accept the current state as the new
baseline. The hashes are change detectors, not integrity protection; the check compares source text with nodes only.

## Round trip

Imported gradients/text export through the normal SVG exporter; re-importing that export yields the same gradient stops and text
(tested in `crates/splinely_document/tests/svg_import_report.rs`). Geometry is baked into paths, so structure (not original element
types, `<use>`, ids of non-groups) is not preserved.
