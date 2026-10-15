# Editing path anchors and tangents

Select an unlocked, visible path and enable **Path handles** on the canvas.
Rectangles and circles can first be converted to paths in the Vector inspector.
Editing is disabled during animation/runtime preview and playback.

- Squares are anchors; circles are Bézier controls. Drag normally to move a handle independently.
- **Shift-drag a control** to reflect its existing opposite control across the shared anchor (symmetric: same direction line and length).
- **Shift+Alt-drag** to align the opposite control while retaining its original length (smooth).
- Open endpoints and joins with lines have no opposite control: no handles are invented. A quadratic's single control affects both endpoint tangents, so linking it updates existing neighbors at both ends. Closing seams link only when the final curve ends at the first anchor; a nonzero `Z` edge is a line.
- **Double-click within 8 screen pixels of an edge**, away from its endpoints, to insert an anchor. Lines, quadratics and cubics are split with De Casteljau, preserving their geometry. A closing edge becomes a line to the new anchor followed by `Z`; separate subpaths stay separate. An endpoint or a miss does nothing.
- **Escape** cancels a handle gesture. Each completed drag or insertion is one undoable change. Canvas panning takes priority over path editing.

Picking and insertion use the evaluated scene transform (including bones), canvas origin and zoom. A drag freezes that mapping for its duration. Mirrored transforms work; singular/zero-scale transforms reject edits. Modifiers can change during a drag; linking always uses the original handles, not accumulated previous preview updates.

Edits are staged and validated, including path bounds and the whole project, before commit. Failed edits never mutate the live project. Actual path changes recompute bounds and remove `originalKind` shape-restoration provenance; unchanged drafts keep it. The original `d` text is retained until an actual edit. Graphical editing normalizes shorthand and arcs to explicit renderer geometry at renderer f32 precision; splitting preserves that parsed geometry, not an analytic SVG arc before normalization.

These are gesture-time linking operations, **not automatic tangent modes stored in the document schema**. Raw SVG `d` editing remains available.

## APIs

In `crates/splinely_editor/src/vector_tools.rs`:

- `EditablePath::insert_nearest(screen, &CanvasMap, max_distance)` returns `Result<Option<EditablePath>, String>`: a validated staged path, a bounded miss/endpoint, or an error. It never mutates the source.
- `EditablePath::split_segment(index, t)` returns a staged split for an interior parameter `0 < t < 1`.
- `EditablePath::move_tangent(handle, point, TangentLink::{Independent, Smooth, Symmetric})` validates before replacing the editable path.
- `PathGesture::update_linked` updates a preview from frozen original geometry; `candidate` also rejects stale documents.
- `PathGesture::insert_candidate` stages a fully validated project for the caller's single history commit.
