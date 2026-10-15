# Audio workflow

## Author cues

In **Animate → Audio**, enter a local audio path and choose **Embed audio**.
Supported sources: WAV, MP3, Ogg, M4A/MP4, FLAC, WebM. Import preserves the
compressed source bytes in the project; it does not transcode or verify decoding.
Browser/ffplay codec support still determines playback.

Select an asset, then choose **Global timeline** or a clip in the cue scope
dropdown. **Add audio cue here** creates a cue in that scope. For a clip, the
current ruler time is used as a local time, without wrapping. Select that clip
in the timeline to position its playhead; insertion is disabled outside the
clip's [0, duration) range, including exactly at its end. Edit the asset, local
start time, source offset, volume (0–1), and optional positive duration. Remove
with ×. Changes use the existing document validation and undo/redo workflow.
A clip cue must start before the end of its clip. IDs are globally unique,
including global cues and cues in other clips.

Clip cues serialize as `clips[].audioCues`, using the same `AudioCue` fields
as `Project.audioCues`. Old clips default to no cues. The browser runtime already
schedules these cues on each clip's local clock: play/pause, seek, and looping
follow the clip, independently of global cues. Visual layer weights do not
attenuate audio. Test final scheduling in the interactive browser export.

Native ffplay preview plays **global and clip-local cues**. Enable/reload native
preview after document changes. Clip audio follows the editor's visual preview:
selected-clip transport normally, or independently playing layers in blend
preview. Paused/inactive layers are silent; visual weights do not attenuate
sound. Cues resume from the current source offset after pause/seek and are
truncated at the clip end. Forward discontinuities and whole skipped loops are
resynchronized using the editor's transport tick, not just decreasing local
time. Preview remains frame-driven: sounds entirely between GUI updates can
be missed, and ffplay startup is not sample accurate. State/action sounds are
not part of native editor preview.

The Rust runtime emits `clipAudioCue` host actions with `clipId` and `cue`.
`cue.time` stays clip-local; `offset` and `duration` describe the remaining
portion at a start/resume/playing seek, bounded by the clip end. Each clip has
an independent cue clock, including time-zero cues on every loop. Paused seeks
are silent; playback resumes overlapping cues without replaying expired cues.
Global cues retain their existing `audioCue` actions and silent seek semantics.
Large ticks split at clip cue and loop boundaries, subject to the existing
segment/output limits and atomic request rollback.

Rust runtime actions are **host notifications, not an audio engine**. Hosts must
cancel voices for the affected clip before pause/stop/seek, cancel all relevant
voices on global pause/seek, and honor emitted remaining durations. They must
not cancel unrelated clips on clip-local commands. No JavaScript runtime source
changes are required for browser clip cues.

## Size and memory

The panel reports exact source bytes per asset, total source bytes, and total
base64 bytes. Source bytes are compressed file size, **not decoded PCM memory**.
New imports and external bundles enforce 32 MiB per source and 128 MiB total;
existing documents are not silently modified to fit these limits. Empty imports
and nonregular files are rejected (including FIFOs without blocking). Import reads at most 32 MiB + 1 byte, even
if the source grows after opening.

Decoded float PCM is roughly `seconds × channels × sample rate × 4` bytes
(e.g. one stereo minute at 48 kHz is about 22 MiB), plus decoder/runtime overhead.
The editor cannot infer or enforce a decoded memory ceiling without decoding.
The browser runtime limits base64 to 64 MiB per asset and extra padded-loop
buffers to 128 MiB; those are **not a total decoded-audio memory limit**.
Native preview has a 128 MiB source cache, 64 assets, 4096 cues across all scopes, and
8 simultaneous voices. Long sources should be trimmed before import.

## Default: offline and self-contained

Save projects and use the ordinary interactive SVG export as before. All audio
remains embedded; there is no fetch and no external dependency. Browser audio
still requires a user gesture using the exported start control. Custom project
JavaScript remains disabled unless its separate trust option is enabled.

## Explicit external-audio bundle

Expand **Export external audio files (optional)** in the Audio panel and click
**Create new external audio export**. This does not change the project, ordinary
export settings, or existing embedded exports. The destination is the configured
SVG export path with `.audio` appended, e.g. `scene.svg.audio/`:

```text
scene.svg.audio/
  animation.svg
  audio-0.wav
  audio-1.mp3
```

Only generated ASCII index filenames and MIME-derived extensions are used;
asset IDs/names never become paths or URLs. Directory creation is exclusive:
existing files, directories (even empty), and symlinks are refused. Every file
uses create-new semantics and the SVG is written last. This is atomic-ish, not a
filesystem transaction: a failed write or process crash can leave an incomplete
directory, which must be inspected and removed or renamed before retrying.
Partial output is deliberately retained: recursive cleanup through a pathname
that another process could replace would risk deleting unrelated files. No files
are overwritten or automatically deleted.

Keep sidecars next to `animation.svg` and serve the directory over HTTP(S).
No internet connection is needed for a local HTTP server, but `file://` fetch,
CSP restrictions, SVG-as-image embedding, or script-disabled viewers can prevent
playback. Missing sidecars retain the static SVG and report an error to the
browser console. The generated opt-in loader fetches only those fixed relative
filenames, with bounded streaming reads (32 MiB each / 128 MiB total), hydrates
embedded metadata in memory, then inserts the unmodified bundled runtime and
normal browser host controls. Audio still starts only after a user gesture.
External export reduces the SVG's disk size, **not peak playback memory**;
hydration temporarily holds strings and decoded data. There are no remote URLs
in project audio assets, and no JS runtime file edits or new network behavior
in default exports.

## Integration notes

The new panel labels use the existing German source-string translation fallback.
Translations for cue scope/help, limits, external export disclosure and button,
and the exact-byte asset label are required in locale JSON by the integrating
worker; no locale JSON is changed by this audio implementation.
