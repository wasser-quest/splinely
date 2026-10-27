# Editing path anchors and tangents

Select an unlocked, visible path and enable **Path handles** on the canvas.
Rectangles and circles can first be converted to paths in the Vector inspector.
Editing is disabled during animation/runtime preview and playback.

- Hollow markers are anchors, filled accent markers are **selected** anchors; the hovered marker grows. Corner anchors are squares, smooth ones circles, symmetric ones diamonds. Small circles are Bézier controls; they are shown only on the segments next to selected anchors, so select an anchor first to reach its handles.
- **Click** an anchor to select it, **Shift+click** to add/remove it, **drag on empty canvas** for a rubber-band selection (Shift adds), click empty canvas or press **Escape** to clear it.
- **Dragging an anchor** moves it together with its tangent handles; dragging a selected anchor moves every selected anchor. Dragging a control moves only that control (unless linked, see below).
- **Shift-drag a control** to reflect its existing opposite control across the shared anchor (symmetric: same direction line and length).
- **Shift+Alt-drag** to align the opposite control while retaining its original length (smooth).
- Open endpoints and joins with lines have no opposite control: no handles are invented. A quadratic's single control affects both endpoint tangents, so linking it updates existing neighbors at both ends. Closing seams link only when the final curve ends at the first anchor; a nonzero `Z` edge is a line.
- **Double-click within 8 screen pixels of an edge**, away from its endpoints, to insert an anchor. Lines, quadratics and cubics are split with De Casteljau, preserving their geometry. A closing edge becomes a line to the new anchor followed by `Z`; separate subpaths stay separate. An endpoint or a miss does nothing.
- With selected points: **Delete/Backspace** deletes them (neighbouring segments merge, keeping the outer controls; subpaths with fewer than two anchors are dropped; deleting every point deletes the shape), **arrow keys** nudge them (Shift: large step), **Ctrl+C / X / V** copies/cuts/pastes runs of adjacent selected points as new subpaths of the edited path (an SVG path `M…` in the system clipboard is pasted as well), **Ctrl+A** selects all points.
- Node commands (keymap, command palette and right-click menu): **Ctrl+1** split tangents (corner, independent handles), **Ctrl+2** smooth, **Ctrl+3** symmetric, **Ctrl+4** automatic tangents, retract/extend handles, **Ctrl+B** break the path at the selected points, **Ctrl+J** join two selected open endpoints, segment to line/curve (between two selected neighbours), reverse path direction. Shortcuts are rebindable in the settings.
- **Pen:** Backspace removes the last anchor (or reopens a closed path); clicking the first anchor closes and finishes the path.
- **Escape** cancels a handle gesture. Each completed drag or insertion is one undoable change. Canvas panning takes priority over path editing.

Picking and insertion use the evaluated scene transform (including bones), canvas origin and zoom. A drag freezes that mapping for its duration. Mirrored transforms work; singular/zero-scale transforms reject edits. Modifiers can change during a drag; linking always uses the original handles, not accumulated previous preview updates.

Edits are staged and validated, including path bounds and the whole project, before commit. Failed edits never mutate the live project. Actual path changes recompute bounds and remove `originalKind` shape-restoration provenance; unchanged drafts keep it. The original `d` text is retained until an actual edit. Graphical editing normalizes shorthand and arcs to explicit renderer geometry at renderer f32 precision; splitting preserves that parsed geometry, not an analytic SVG arc before normalization.

Linking and node types are geometry operations, **not tangent modes stored in the document schema**: the node type shown is inferred from the current handles. Raw SVG `d` editing remains available.

## APIs

In `crates/splinely_editor/src/vector_tools.rs`:

- `EditablePath::insert_nearest(screen, &CanvasMap, max_distance)` returns `Result<Option<EditablePath>, String>`: a validated staged path, a bounded miss/endpoint, or an error. It never mutates the source.
- `EditablePath::split_segment(index, t)` returns a staged split for an interior parameter `0 < t < 1`.
- `EditablePath::move_tangent(handle, point, TangentLink::{Independent, Smooth, Symmetric})` validates before replacing the editable path.
- `PathGesture::update_linked` updates a preview from frozen original geometry; `candidate` also rejects stale documents.
- `PathGesture::insert_candidate` stages a fully validated project for the caller's single history commit.

The pure point-editing geometry (selection hit-testing priority, marquee, translate/delete/node types/break/join/segment conversion/reverse/copy/paste) lives in `splinely_document::path_edit` (`crates/splinely_document/src/path_edit.rs`) and is unit-tested there. Undo history keeps the newest steps when the step or memory limit is reached (`History::set_limits`).
