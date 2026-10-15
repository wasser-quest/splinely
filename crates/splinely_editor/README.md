# Splinely desktop editor

Run from the workspace:

```sh
cargo run -p splinely_editor --bin splinely
cargo run -p splinely_editor --bin splinely -- --demo-export /tmp/demo.svg
cargo run -p splinely_editor --bin splinely -- --export project.json output.svg
cargo check -p splinely_editor
cargo test -p splinely_editor
```

The desktop opens a working demo. Create rectangles, circles, and text; select objects on the canvas/list, drag them, and edit geometry, transforms, fill/stroke, opacity, and text in the inspector. Undo/redo use the document core's validated commands. A drag or continuous inspector/keyframe gesture is coalesced into one undo transaction. Ctrl/Cmd-Z, Shift-Ctrl/Cmd-Z, Ctrl/Cmd-S, and Space work outside text fields. The snapshot history is bounded to approximately 128 MiB/100 transactions; older history is cleared when that budget is reached.

Save/load use the project path field. Save writes a temporary file and renames it. Dirty projects also save to `PROJECT_PATH.autosave` every 30 seconds (load that path explicitly to recover). Load is undoable. Interactive SVG embeds the runtime via `include_str!("../../../runtime/src/splinely.js")`; static SVG materializes the current native preview. No file picker or server is needed.

## Timeline

Select an object to add a property track. Tracks expose editable times, numeric/color values, linear/step/cubic Bézier easing, copying, removal, and key insertion at the playhead. Named clips have duration, looping and autoplay settings. Scrub/play/pause/rewind evaluate immutable document data; the native preview selects one clip at a time. **Evaluate clip** switches between evaluated and base-scene editing; beginning a canvas drag disables animation evaluation so it does not mask edits. The Timeline toolbar checkbox collapses the timeline.

## Bones and navigation

Open **Bones** to add a root, select it, and add children. Edit parent, local X/Y, rotation (degrees), and positive length in the sidebar; child offsets start at the parent tip. Joint guides are selectable, editor-only overlays (joint dragging is not implemented). Select an object in Layers and use **Bind object** or **Unbind**. Binding preserves the base world transform; remove object X/Y/rotation tracks and state actions/overrides first to avoid changing their coordinate system. Locked objects cannot be edited, dragged, or rebound.

Create/select a clip in the timeline, key the local bone pose at zero, move the playhead, change local values, and key again. Disable **Evaluate clip** to see base-pose edits. Bone tracks expose the existing timeline key times/values/easing controls. Rig edits and keys use validated undo history; invalid parents/cycles/lengths leave the document untouched. Static snapshots materialize bindings only once.

**F/Home** fits the scene and resets scrolling, **+/-** zooms, **0** restores 100%, and wheel/Shift-wheel navigate the scrollable canvas. Initial view fits the scene. Layer controls provide visibility, locking, ordering, duplication, and validated deletion; vector controls provide conversion and path editing. Native raster preview uses resvg; hidden layers do not render.

## States, audio, code

- **States:** displays active state boxes and transition descriptions. The editable machine JSON is the real schema, including states, transition events/targets, guards, entry actions and property overrides. Apply validates before changing the document. Variables have their own scalar-value JSON editor. Send events or click canvas objects to exercise the native state preview. Properties, variables, emitted events and selected-clip playback are supported; event recursion is bounded. Reset reconstructs initial states and entry actions. Audio actions are logged, not played natively.
- **Audio:** import local wav/mp3/ogg/m4a/flac/webm files (32 MiB maximum) as base64 assets; edit scheduled audio cues and speech cues. Browser codec support, audio unlock, and available speech voices are platform-dependent. Native preview is intentionally silent. Exports add a user-gesture Start control; authored speech cues/Speak actions opt into browser synthesis.
- **JavaScript:** editable user code is saved and exported. It is never executed natively. By default it is preservation-only, matching the shared runtime. The explicit **Execute trusted user JS** checkbox appends an editor-owned browser bootstrap that compiles user code with a local `splinely` API binding. This choice is session-only, deliberately not trusted automatically on project load. CLI export also defaults to preservation-only; `--trusted-js` explicitly enables authored code. There is no native JS syntax compiler; browser diagnostics/console report syntax/runtime failures. CSP may block dynamic compilation. Only enable scripts you trust.
- **Raw SVG:** XML/root/duplicate-ID validation before applying. Source is preserved in project and export metadata, **not imported into scene nodes or injected into the rendering**. This is preservation-only source editing, not arbitrary SVG roundtripping.

## Architecture and limits

`app.rs` hosts UI/commands/transport; `panels.rs` implements inspectors and editing tools. `export.rs` implements the optional trusted-code host bootstrap; `ecs.rs` defines the public static `EditorPlugin` trait, ordered Input → Behavior → Animation → Compose → Render phases, and real StateMachinePlugin, TimelinePlugin and ScenePlugin systems. The behavior phase builds the core evaluation context; animation evaluates through `Project::evaluate_with_context`; composition reconciles stable document IDs with Bevy entities; render collects document-order scene data. Deleting nodes removes their dependent tracks/actions/targeted transitions in one validated edit. ECS IDs are disposable and never serialized.

Native preview uses resvg for rect/circle/text/path geometry and transforms, with inverse-transform hit testing (paths use their local bounds). The toolbar imports supported SVG geometry into editable nodes. No group hierarchy editor, multi-clip mixing UI, native audio engine, native JS execution, or untrusted browser sandbox is claimed. Interactive SVG must be opened in a script-enabled browser SVG context, not an `<img>` viewer. Browser runtime availability is not a security guarantee for user JS.
