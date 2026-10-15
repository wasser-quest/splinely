# Native Splinely model runtime

`crates/splinely_runtime` is a browser-independent Rust library (`rlib` and `cdylib`)
and a `splinely-runtime` JSON-lines executable. It loads canonical schema-1 Splinely
JSON through `splinely_document`, advances host-supplied clocks, processes machines,
and returns evaluated nodes/bones plus host action requests.

**This is not a renderer, audio engine, JavaScript interpreter or general SVG
runtime.** No window, DOM, hit testing, audio device or browser is required. Raw
SVG and `customJs` remain inert project data. There is no arbitrary SVG JavaScript
support or automatic conversion of browser callbacks to native code. Hosts supply
input targets, render evaluated geometry and decide whether to honor audio
and custom callback output.

See [CORE_API.md](../CORE_API.md), [BLENDING_SPEC.md](../BLENDING_SPEC.md) and
[BONES_SPEC.md](../BONES_SPEC.md) for model/evaluation contracts. This runtime uses
core evaluation, not a second implementation of track sampling, weighted channel
blending or FK. Evaluated nodes are already bone-attached; do not attach twice.

## Build and verify

The crate is registered in the root workspace. A local Linux x86-64 optimized,
stripped build measured approximately **1.4 MiB for the shared library** and
**1.5 MiB for the JSON-lines executable** (file sizes, not RAM/latency benchmarks).
No GTK/WebKit dependency is required for this crate. From the repository root:

```sh
export TMPDIR="$PWD/.work-tmp"
cargo test -p splinely_runtime
cargo test --workspace
cargo build -p splinely_runtime
python3 examples/native-runtime/smoke.py \
  --library target/debug/libsplinely_runtime.so \
  --binary target/debug/splinely-runtime
python3 examples/native-runtime/ctypes_runtime.py target/debug/libsplinely_runtime.so
```

Use `libsplinely_runtime.dylib` on macOS or `splinely_runtime.dll` on Windows (and
`splinely-runtime.exe` for the binary). Paths may differ with a custom Cargo target
folder. The Python example has no third-party dependencies. Other languages can
use their FFI mechanism or a JSON-lines subprocess.

Root workspace tests and Clippy passed after integration. The real Linux shared
library and executable passed `examples/native-runtime/smoke.py`, including
ctypes ownership, independent handles, state/fade/bone behavior and JSON-lines
parity. Regression tests reject nested unknown fields, duplicate members and
lossy integer payloads through all three JSON entry points. No audible playback,
rendered image, Windows or macOS ABI run was tested.

## Rust API

```rust,no_run
use splinely_runtime::{Runtime, Request};
let mut runtime = Runtime::from_json("{}")?;
let initial = runtime.execute(Request::Evaluate)?; // drains initial host actions
let output = runtime.tick(0.5)?;
let frame = runtime.evaluate()?; // immutable, does not drain output
# Ok::<(), splinely_document::DocumentError>(())
```

Public entry points:

- `Runtime::new(project: Project) -> Result<Runtime>` validates before use.
- `Runtime::from_json(json: &str) -> Result<Runtime>` uses strict core loading.
- `Runtime::project(&self) -> &Project` exposes immutable authored data.
- `Runtime::evaluate(&self) -> Result<Frame>` samples current state without effects.
- `Runtime::execute(&mut self, request: Request) -> Result<Response>` is atomic.
- `Runtime::event(&mut self, event: Event) -> Result<Response>` sends a queued event.
- `Runtime::tick(&mut self, delta: f64) -> Result<Response>` advances seconds.
- `Runtime::request_json(&mut self, json: &str) -> Result<String>` accepts one
  serialized `Request` and returns a serialized `Response` (not an ABI envelope).

`Result<T>` uses `splinely_document::DocumentError`. `Response` contains `frame` and
`hostActions`. `Frame` contains `time`, `running`, `states`, `variables`, `clips`,
`nodes`, `bones`; each clip reports `id`, `time`, `active`, `playing`, `weight`.
Private fade state is not serialized. Successful execute/event/tick calls drain
host output exactly once. `new` runs entries once and retains their host output
until the first successful execute; immutable evaluate does not consume it.
Failed operations retain the previous state/output: the implementation applies
requests to a clone and commits only after successful evaluation.

## Request JSON

The Rust enum has the following camelCase wire form. Unknown request fields are
rejected recursively, including inside actions/events; duplicate object members,
oversized integer tokens and typed numeric precision loss are rejected before
mutation. Callback data retains arbitrary user keys. Actions and events use
canonical core types.

| Request | Fields besides `type` |
| --- | --- |
| `evaluate` | none |
| `tick` | `delta` |
| `event` | `event: {name,target?}` |
| `action` | `action: <core Action>` |
| `playClip`, `pauseClip`, `stopClip` | `clipId` |
| `seekClip` | `clipId`, `time` |
| `setWeight` | `clipId`, `weight` |
| `fadeClip` | `clipId`, `weight`, `duration` |
| `play`, `pause` | none |
| `seek` | `time` |
| `clearOverrides` | none |
| `schedule` | unique nonempty `id`, `time`, optional `data`, optional `event` |
| `cancelCallback` | `id` (missing registration is a no-op) |

All times/deltas/durations must be finite and nonnegative; weights are finite
[0,1]. Unknown IDs, wrong scalar variable types and invalid node/bone patches
fail rather than silently disappearing. `action` accepts core actions only:
setProperty, playClip, stopClip, blendClip, setVariable, emit, playAudio.

## Deterministic native transport

- Timeline starts running at time zero. A host tick is the only clock source.
  `pause` freezes timeline and clip samples; `play` resumes them. This explicit
  native clock differs from the browser's implicit timeline/audio clocks.
- Autoplay clips start active/playing. Active clips evaluate in **document order**,
  regardless of activation order. PauseClip retains selection and time. StopClip
  deactivates, resets local time, cancels its ramp and retains its current weight.
  PlayClip is idempotent while playing; finished nonloops restart at zero.
- Local seeks wrap loops and clamp nonloops, do not activate clips, and do not
  change fade clocks. Global seek silently synchronizes active clips and rearms
  future cues/callbacks. Entries at/before the seek destination are skipped;
  seek does not emit completion events or recreate overlapping audio.
- Nonloops retain the final pose and enqueue `clip.end` targeted to their ID once
  at natural completion. Loops wrap and enqueue `clip.loop` at each boundary.
  A seek to the nonloop endpoint remains silent until a subsequent positive tick
  processes completion for a still-playing clip. Decimal boundary rounding within
  eight local-duration floating-point epsilons is snapped to the endpoint.
- Linear fades start from the synchronized current weight. Interrupted ramps start
  there too. Zero-duration fades are immediate. Fades advance on every tick even
  while a clip/timeline is paused, ended or inactive. Fade-to-zero does not stop
  or activate. FadeClip/core blendClip require Weighted mode; setWeight can update
  any clip but Override evaluation ignores that weight. Visual weights do not
  affect requested audio volume. The authored project remains unchanged.
- Tick splits at global cue/callback and clip loop/end boundaries. Actions at a
  boundary affect the remaining segment (for example, a callback event starting
  another clip at 0.5 in a one-second tick advances it by only 0.5 seconds).
  Fade weights are synchronized before boundary actions. Excessive ticks or times
  that cannot advance at f64 precision fail atomically, not by hanging.
- All initial entries execute in machine order before emitted events drain.
  Machines share variables; each machine sees prior machines' assignments.
  First matching transition wins; transition actions precede destination entry.
  State override precedence remains core machine order, then clips, then explicit
  setProperty patches. ClearOverrides removes those explicit patches.
- Events are FIFO, not recursive. At equal times, global audio cues,
  then custom callbacks are collected (document/registration order). Callback
  events and clip boundaries (document order) are queued before draining; nested
  emitted events append after the already queued boundary batch. Host output is
  delivered after the complete operation, not as synchronous native callbacks.

## Host actions and custom callbacks

HostActions use a `type` tag:

- `event {event}`: input, emitted or clip-boundary event, for host subscribers.
- `playAudio {assetId}`: state/action request.
- `audioCue {cue}`: full authored global cue parameters.
- `callback {id,data}`: registered native-host callback request.

Global cues cross once in transport time, independent of visual clip loops.
Time-zero cues are delivered on the first running tick (including tick(0));
construction/evaluate do not fire cues. Global pause is silent. Backward seek
rearms future crossings; forward seek skips crossings. Audio output is only data:
no decoding, synthesis, scheduling ahead, stop/resume engine or latency guarantee.
A host building those features must manage its own playback lifetimes.

`schedule` registers a one-shot global callback as an **ID and JSON data**, not a
function or JavaScript source. An optional canonical event is dispatched at its
boundary so authored actions can affect the remainder of that tick. A host may
map the returned ID to its own trusted language function after processing the
response. Such host functions cannot synchronously mutate a tick that has already
completed. Callbacks rearm on backward seek and retain their IDs until canceled;
registration in the past is inert. There are no clip-local callback registrations.

## Bounds and C ABI

Limits per operation: 256 dispatched/queued events, 4096 retained host outputs,
4096 tick segments. At most 4096 callbacks may be registered. Exceeding limits
returns an error and rolls back the request; initialization cycles reject creation.
These are execution bounds, **not** full document/resource quotas. Rust typed
loading has no new global asset-size limit. C inputs and JSON-lines records are
limited to 64 MiB each; overlong lines are discarded with one error response.

Include [`splinely_runtime.h`](../crates/splinely_runtime/include/splinely_runtime.h):

```c
char *splinely_runtime_create(const uint8_t *data, size_t length,
                          SplinelyRuntimeHandle **out);
char *splinely_runtime_request(SplinelyRuntimeHandle *handle,
                           const uint8_t *data, size_t length);
void splinely_runtime_destroy(SplinelyRuntimeHandle *handle);
void splinely_runtime_result_free(char *result);
```

Inputs are byte-length UTF-8 JSON, not necessarily NUL terminated. Returned
strings are separately owned NUL-terminated UTF-8 JSON envelopes:
`{"ok":true,"result":<Response>}` or `{"ok":false,"error":"..."}`. Free **every**
returned allocation exactly once with result_free, never libc free or a language
allocator. Strings outlive their runtime. Create writes a new opaque handle to a
fresh output slot, or NULL on failure. Destroy each successful handle exactly once;
NULL destroy/free are no-ops. Python must use `c_void_p` result pointers, not
`c_char_p` automatic conversion, to preserve the pointer for freeing.

Null inputs, invalid UTF-8/JSON, and unwinding runtime panics are diagnosed. Rust
panic guards do **not** validate native memory. The caller must provide readable
input bytes, a disjoint writable/aligned output pointer slot, live handles, and
live unmodified result allocations. Forged/dangling/undersized pointers, use after
free, double free and concurrent access to one handle are caller undefined
behavior. Serialize calls to each handle; independent handles share no runtime
state. Allocation failure or process abort is not recoverable. The header is the
explicit allocation/free and pointer-validity contract.

### Buffer API (no per-call allocation)

Additive to the four functions above (same ABI v1, nothing removed). Hot loops can
write the response envelope into a caller buffer instead of receiving an allocated
string:

```c
int32_t splinely_runtime_request_into(SplinelyRuntimeHandle *handle, const uint8_t *data,
                                      size_t length, uint8_t *out, size_t capacity,
                                      size_t *out_length);
int32_t splinely_runtime_tick_into(SplinelyRuntimeHandle *handle, double delta,
                                   uint8_t *out, size_t capacity, size_t *out_length);
int32_t splinely_runtime_response_copy(SplinelyRuntimeHandle *handle, uint8_t *out,
                                       size_t capacity, size_t *out_length);
```

The envelope is the same document as for `splinely_runtime_request`, UTF-8 and
**not** NUL terminated; object key order is unspecified. `*out_length` always gets
its size. Status `0` means it was copied (it may be an `{"ok":false,...}` error
envelope), `1` (`SPLINELY_STATUS_BUFFER_TOO_SMALL`) means the buffer was too small:
the request **has already executed** and the handle retains the response, so call
`splinely_runtime_response_copy` with a buffer of `*out_length` bytes rather than
resending, and `2` means a null argument (nothing executed). `tick_into` skips
request JSON parsing. The same memory contract as above applies.

## Evaluation cost

The project is validated once in `Runtime::new` (which builds a
`splinely_document::EvalIndex` of ID to position tables); ticks, `evaluate` and
`SetProperty` validation use `Project::evaluate_frame`, which evaluates nodes and
bones in one pass without re-validating or scanning IDs. The public
`Project::evaluate*` functions still validate their (possibly unvalidated) input
and give identical results. The project is shared by `Arc`, so the per-request
rollback snapshot copies only the mutable playback state. Machines are only asked
to fire when one of their transitions can match the event/timer. Measure with:

```sh
cargo run --release -p splinely_runtime --example tick_bench [project.json ...]
```

## Viewer: starting this runtime (`splinely-viewer --runtime rust`)

`splinely-viewer <project> --runtime js|rust` chooses who animates the project in the
native window. `js` (default) runs `runtime/src/splinely.js` in the WebView. `rust` links
this crate into the viewer process: `Runtime::new(project)`, a fixed 1/60 s `Request::Tick`
from the event loop, and the resulting `Frame` is applied to the static SVG
(`Project::to_static_svg(0.0)`) by a small dependency-free page script using the JS runtime's
attribute formats. WebView pointer/keyboard events return over a validated, size-limited IPC
and become `Pointer`, `PointerLeave` and `Event` requests. Details, limits and the IPC schema:
[crates/splinely_viewer/README.md](../crates/splinely_viewer/README.md).

Not reproduced in rust mode: `customJs` (never executed; `--trusted-js` is refused), all audio
(assets, `playAudio`, audio cues, no unlock control) and callback host actions, reduced-motion
handling, accessibility semantics (roles, `tabindex`, `aria-*`, keyboard activation, focus ring, live region – see [accessibility.md](accessibility.md)), DOM event details beyond name and target, SVG (non-project) input. The viewer shows a
visible diagnostic for the applicable items and falls back to the JS runtime (with the reason) if
`Runtime::new` rejects the project. Parity of applied attributes with the JS runtime is tested to
1e-9 for several examples (`crates/splinely_viewer/tests/rust_runtime.rs`).

## JSON-lines process protocol

Send one UTF-8 JSON object per stdin line; stdout returns exactly one envelope per
line, flushed immediately. A final unterminated line is accepted. Begin with:

```json
{"type":"load","project":{}}
{"type":"tick","delta":0.5}
{"type":"event","event":{"name":"click","target":"box"}}
{"type":"evaluate"}
```

`load` is a process-only command replacing the current runtime after successful
strict core loading/initialization. A failed reload preserves the old runtime.
All subsequent records use the Request table above. Invalid records (including
invalid UTF-8) return errors without breaking framing or ending the process. No
CLI banners are written to stdout; I/O/process failures remain process failures.
The real subprocess smoke compares results against the shared-library API.

**Accessibility data:** the optional project `description`, node `a11y` and state
`announce` fields are validated and preserved by strict loading, but the native
runtime ignores them (it has no DOM, focus or live region). Screen-reader
semantics exist only in the browser web player, see
[accessibility.md](accessibility.md).
