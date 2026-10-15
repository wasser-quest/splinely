# Splinely authored project files (`.sply`)

`.sply` is the recommended extension for a **single, self-contained authored
Splinely Project**. It contains UTF-8 JSON using the existing canonical Project
schema (`schemaVersion: 1`). It is honestly a JSON-based format: no ZIP archive,
magic header, binary codec, sidecar SVG, or additional container version.
`.splinely.json` and `.json` remain compatible. Loading is content-based regardless
of extension; renaming a valid JSON Project to `.sply` works. Exported SVG is a
presentation artifact, not an authored Project and not an input to `convert`.

## Contents and fidelity

The Project stores geometry and styling, layers, transforms, animations and
blend settings, states and actions, bones and bindings, variables, embedded
base64 audio and cues, `customJs`, and `rawSvg`. No adjacent JSON,
SVG or audio files are required to retain this authored data. This does **not**
mean a `.sply` file renders or executes itself: a compatible host must load it,
or the CLI must export an SVG for a script-capable browser host.
Embedded audio playback still depends on host support and user gestures.

Saving produces canonical pretty JSON with a trailing newline. Existing
omitted default fields are materialized; original whitespace, key order, number
spelling and SVG source formatting are not guaranteed. Authored supported values
are preserved. `rawSvg` remains metadata, not injected/rendered geometry.
Loading and conversion do not execute `customJs`; its presence never grants
trust. Browser export requires an explicit `--trusted-js` to run reviewed code.
No native plugin is loaded from this format.

## Strictness and limits

The same strict JSON and validation rules apply to `.sply` and legacy JSON:
unknown typed fields, duplicate members (also nested), integer tokens outside
the signed/unsigned 64-bit range, lossy integer-to-model-float conversions,
unsupported schema versions and invalid references/constraints fail. Missing
model defaults remain allowed. Decimal numbers use IEEE-754 double precision.
There is no permissive migration that silently drops unknown authored data.

Shared file APIs limit encoded input and output to **64 MiB**, including
whitespace and the saved newline. Reads stop after at most limit + 1 bytes;
serialization writes through a bounded writer without allocating an entire
serialized output in memory. This is a byte limit, not an execution sandbox or
a bound on all parser/model allocations. CLI JSON project, patch and context
inputs use the same limit; project stdout output is also size-checked. Exported
SVG has a different representation and is not subject to the project-file limit.
In-memory `from_json`/`to_json` retain their existing behavior.

## Atomic saves

The parent directory must already exist. Saving validates, exclusively creates
a private temporary file in the destination directory, writes bounded canonical
JSON, syncs the file, closes it, then atomically renames it over the destination.
Failure before commit leaves the old destination unchanged and cleans up the
temporary file on ordinary error paths. In-place edits and conversion are safe.
A process crash may leave a temporary file. No directory sync, multi-writer
locking, original-permission preservation or power-loss durability is promised.
A failed acknowledgement on stdout after commit does not undo a saved file.
Callers must serialize competing edits. Filesystem/OS rename restrictions can
cause a save to fail rather than falling back to a non-atomic overwrite.

## Shared document API (main/editor/viewer integration)

Exported by `splinely_document`, with `DocumentError` implementing `Display` and
`std::error::Error`:

```rust
pub const MAX_PROJECT_FILE_BYTES: u64 = 64 * 1024 * 1024;

impl Project {
    pub fn load_file(path: impl AsRef<std::path::Path>) -> Result<Self, DocumentError>;
    pub fn save_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), DocumentError>;
}
```

For example, a host can call `Project::load_file(path)?` and hand the returned
Project to its existing document pipeline, then call `project.save_file(path)?`.
These synchronous filesystem methods do not handle dialogs, autosave policies,
history, rendering, evaluation or trust decisions. `-` is a literal path at this
API boundary; stdin/stdout interpretation belongs only to the CLI. File errors
include path context; validation errors remain document errors.

This change is scoped to document file I/O, CLI and documentation. Editor/viewer
file pickers, associations and startup loading have **not** been changed. Model,
validation, editor, viewer and runtime semantics are unchanged. The format adds
no network access, archive extraction, external-resource resolution or untrusted
resource handling. Enabling authored JavaScript in a host can itself permit
network access and must remain an explicit host trust decision.

## CLI workflow

```sh
splinely-cli new --demo --output scene.sply
splinely-cli example --output example.sply
splinely-cli import-svg drawing.svg --output drawing.sply
splinely-cli validate scene.sply
splinely-cli keyframe scene.sply --output scene.sply --clip motion --target ball --property x --time 0.5 --value 120
splinely-cli export scene.sply --output scene.svg
splinely-cli convert old.splinely.json --output scene.sply
splinely-cli convert scene.sply --output interoperable.json
```

`convert INPUT --output PATH` validates and canonicalizes an authored Project;
there is no separate `--to` switch because both extensions use the same JSON
schema. It supports in-place replacement, input `-` and `--output -` under the
existing CLI JSON protocol. `example` without `--output` still emits Project JSON
on stdout. Other authoring commands use `.sply` paths unchanged. Outputs remain
explicit: no inferred export path or automatic overwrite of a sibling file.
See [CLI.md](../CLI.md) for commands and host/export limitations.
