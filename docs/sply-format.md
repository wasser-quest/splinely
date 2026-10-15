# Splinely authored project files (`.sply`)

`.sply` is the **single, self-contained authored Splinely Project**. A `.sply`
file is written as **compact binary** by default: a lossless structural encoding
of the canonical Project JSON model (`schemaVersion: 1`). It is not a second
schema, not a ZIP archive and has no sidecar files. The byte-level layout is
specified in [sply-binary-format.md](sply-binary-format.md).

`.splinely.json` and `.json` stay **readable canonical JSON text**, and legacy
JSON-text `.sply` files written by older versions still open everywhere.
Exported SVG is a presentation artifact, not an authored Project and not an input
to `convert`.

## Binary by default, JSON on request

| Situation | Encoding |
|---|---|
| File name ends in `.sply` (any case): CLI edits/creators, `convert`, editor *Save* | binary |
| `.splinely.json`, `.json`, any other extension | JSON text |
| `convert --format binary` / `--format json` | forced, whatever the extension |
| CLI stdout (`example`, `cat`, `convert … --output -`, `--output -`) | always JSON (made for LLM workflows; binary on stdout is refused) |
| SVG export | project embedded as base64 binary by default; `--embed json` keeps the legacy readable text |

**Loading never depends on the extension.** Every host sniffs the first eight
bytes: `89 53 50 4C 59 0D 0A 1A` (`\x89SPLY\r\n\x1a`) means binary, anything else
is parsed as strict UTF-8 JSON. Renaming a JSON project to `.sply` (or the other
way round) works; re-saving then writes the encoding the *new* extension implies
(a legacy JSON `.sply` is migrated to binary on its next save). CLI commands also
accept either encoding on stdin (`-`).

Why binary: no text parsing, interned strings, raw audio bytes instead of base64
text, packed arrays and a CRC; projects are smaller and load faster. JSON remains
the diffable, hand-editable and LLM-friendly form.

## Contents and fidelity

The Project stores geometry and styling, layers, transforms, animations and
blend settings, states and actions, bones and bindings, variables, embedded
audio and cues, `customJs`, `rawSvg`, and, for SVG imports, `svgImport`. No adjacent files are required. This does
**not** mean a `.sply` file renders or executes itself: a compatible host must
load it, or the CLI must export an SVG for a script-capable browser host.
Embedded audio playback still depends on host support and user gestures.

Binary and JSON describe the same value tree (integer vs float token kinds are
kept), so `convert` round trips in both directions are lossless. Saving
canonicalizes: omitted default fields are materialized, and original whitespace,
key order, number spelling and SVG source formatting are not guaranteed. `rawSvg`
remains metadata, not injected geometry. `svgImport` (optional, omitted when absent) holds
`sourceHash` and `documentHash` (`fnv1a64:` + 16 hex digits) taken at SVG import so that tools can tell
whether `rawSvg` or the nodes changed afterwards; see [svg-import.md](svg-import.md). Loading and conversion never execute
`customJs`; browser export requires an explicit `--trusted-js`. No native plugin
is loaded from this format.

## Strictness and limits

Both encodings go through the same strict typed loading and `validate()`:
unknown typed fields, duplicate members (also nested), integer tokens outside the
signed/unsigned 64-bit range, lossy integer-to-model-float conversions,
unsupported schema versions and invalid references/constraints fail. Missing
model defaults remain allowed. There is no permissive migration that silently
drops unknown authored data.

Binary files are additionally checked structurally before any project rule
(magic, version, size, CRC-32C, string table, UTF-8, tags, varints, floats,
nesting depth, counts, left-over bytes; see the format spec). Hostile input
yields a typed error (CLI code `invalid_binary`), never a panic.

Encoded input and output are limited to **64 MiB** (`MAX_PROJECT_FILE_BYTES`) for
both encodings, files and CLI stdin. Reads stop after at most limit + 1 bytes;
writes go through a bounded writer. This is a byte limit, not an execution
sandbox or a bound on every model allocation. CLI patch and context inputs use
the same limit and must be JSON text. Exported SVG is not subject to the
project-file limit.

## Atomic saves

The parent directory must already exist. Saving validates, exclusively creates a
private temporary file in the destination directory, writes the bounded bytes,
syncs and closes it, then atomically renames it over the destination. A failure
before commit leaves the old destination unchanged and cleans up the temporary
file on ordinary error paths; in-place edits and conversion are therefore safe.
A process crash may leave a temporary file. No directory sync, multi-writer
locking, permission preservation or power-loss durability is promised, and a
failed stdout acknowledgement after commit does not undo a saved file. Callers
must serialize competing edits.

## Shared document API

Exported by `splinely_document` (`DocumentError` implements `Display` and
`std::error::Error`):

```rust
pub const MAX_PROJECT_FILE_BYTES: u64 = 64 * 1024 * 1024;

impl Project {
    pub fn load_file(path: impl AsRef<Path>) -> Result<Self, DocumentError>;             // sniffs binary / JSON
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DocumentError>;                      // sniffs binary / JSON
    pub fn is_binary(bytes: &[u8]) -> bool;
    pub fn save_file(&self, path: impl AsRef<Path>) -> Result<(), DocumentError>;        // JSON text
    pub fn save_file_binary(&self, path: impl AsRef<Path>) -> Result<(), DocumentError>; // binary .sply
    pub fn to_binary(&self) -> Result<Vec<u8>, DocumentError>;
    pub fn to_svg_embed(&self, runtime: &str, embed: EmbedFormat) -> Result<String, DocumentError>;
}
pub enum EmbedFormat { Json, Binary }
```

Hosts use `save_file_binary` for `.sply` and `save_file` otherwise (the CLI and
the editor do this by extension). These synchronous methods do not handle
dialogs, autosave policy, history, rendering, evaluation or trust. `-` is a
literal path at this API boundary; stdin/stdout belongs to the CLI. The editor
and the native viewer open both encodings through this loader; the editor writes
`.sply` as binary and other extensions as JSON (its crash-recovery autosave stays
JSON and is sniffed on load).

## CLI workflow

```sh
splinely-cli new --demo --output scene.sply          # binary
splinely-cli new --demo --output scene.splinely.json # JSON text
splinely-cli node add scene.sply --kind rect --output scene.sply   # in place, stays binary
splinely-cli validate scene.sply                     # also: cat scene.sply | splinely-cli validate -
splinely-cli export scene.sply --output scene.svg    # binary embedding (default)
splinely-cli export scene.sply --output scene.svg --embed json   # legacy readable embedding
```

### `convert`, `cat`, `inspect`

```sh
splinely-cli convert scene.sply --output scene.json                  # binary -> JSON (by extension)
splinely-cli convert scene.json --output scene.sply                  # JSON -> binary
splinely-cli convert scene.json --output scene.dat --format binary   # force the encoding
splinely-cli cat scene.sply                                          # canonical JSON of any project file
splinely-cli inspect scene.sply                                      # summary + file report
```

* `convert INPUT --output PATH [--format json|binary]` validates and
  canonicalizes. Without `--format` the extension decides (`.sply` = binary, else
  JSON). It supports in-place replacement and input `-`; `--output -` prints JSON
  and rejects `--format binary`.
* `cat FILE` prints the project as canonical JSON on stdout, whatever its stored
  encoding; `-` reads stdin.
* `inspect FILE` keeps its project summary and adds `file` and `stats`:
  `file.format` (`binary`/`json`), `bytes`, `schemaVersion`,
  `equivalent.jsonBytes` / `equivalent.binaryBytes` (the size of the same project
  in each encoding), and for binary files `formatVersion`, `stringCount`,
  `sections` (`header`, `stringOffsets`, `strings`, `body` byte sizes), `crc32c`
  and `crcOk` (always `true` for a file that was reported at all: a CRC
  mismatch is a decode failure, so `inspect` errors with `invalid_binary` instead
  of printing `crcOk: false`); `stats` counts nodes, bones, clips, tracks, keyframes, state
  machines, audio assets/cues and followers. A damaged or hostile file produces
  the usual JSON error (code `invalid_binary` for structural problems, or the
  project error such as `unknown_field`) and changes nothing.

Outputs remain explicit: no inferred export path or automatic overwrite of a
sibling file. See [CLI.md](../CLI.md) for all commands and export limits.

## SVG embedding

`<metadata id="splinely-project" data-encoding="sply-base64">BASE64…</metadata>`
holds the base64 of the whole binary file. Without `data-encoding` the content is
legacy JSON text. The JS runtime reads both. Use `--embed json` when the SVG must
stay greppable or a consumer predates binary embedding.
