//! Optional native preview using an installed `ffplay` (FFmpeg), without JS or
//! link-time audio/ALSA dependencies. No manifest changes are required.
//!
//! Integration: declare this module, own an `AudioPreview`, call `set_project`
//! on document changes, `update(time, playing)` every transport frame, and
//! `seek(time, playing)` on explicit seeks/loop wraps (including forward seeks).
//! Drain `take_diagnostics()` into nonfatal UI status. Pause stops voices;
//! resume reconstructs overlaps. Call `stop()` on document close/reset.
//! `set_project` does bounded base64 decoding and disk I/O: run preparation off
//! the GUI thread for large documents, then move the controller to the UI.
//!
//! Global and clip-local cues are supported; action sounds are not.
//! Scheduling is frame-driven, not sample accurate. ffplay startup adds latency;
//! very short cues can be missed. Unknown source lengths are bounded by a five
//! minute preview window. Source duration/codec/device failures are reported by
//! child exit status and bounded stderr, not preflight decoding. The external
//! decoder is NOT a sandbox: use a maintained FFmpeg with trusted documents.
//! Compressed input/cache, voice count, individual FFmpeg allocations and wall
//! time are bounded; total decoder RSS/CPU is NOT a hard OS-enforced quota.
//! ffplay must be on PATH and have a working native audio output (typically SDL
//! with PulseAudio/PipeWire/ALSA on Linux). Missing player/device is optional.

use base64::Engine;
use splinely_document::{AudioCue, Project};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const MAX_ASSET_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_CACHE_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_ASSETS: usize = 64;
pub const MAX_CUES: usize = 4096;
pub const MAX_VOICES: usize = 8;
pub const MAX_PREVIEW_SECONDS: f64 = 300.0;
const MAX_DIAGNOSTICS: usize = 32;
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
struct PreparedCue {
    path: PathBuf,
    scope: Option<String>,
    format: &'static str,
    time: f64,
    offset: f64,
    length: f64,
    volume: f64,
}

struct Cache {
    directory: PathBuf,
    cues: Vec<PreparedCue>,
}
impl Drop for Cache {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn format_for(mime: &str) -> Result<&'static str, String> {
    match mime {
        "audio/wav" | "audio/x-wav" | "audio/wave" => Ok("wav"),
        "audio/mpeg" | "audio/mp3" => Ok("mp3"),
        "audio/ogg" => Ok("ogg"),
        "audio/mp4" | "audio/x-m4a" => Ok("mov"),
        "audio/flac" | "audio/x-flac" => Ok("flac"),
        "audio/webm" => Ok("matroska"),
        _ => Err("unsupported native preview audio MIME type".into()),
    }
}

fn decode(data: &str, remaining: usize) -> Result<Vec<u8>, String> {
    // Check before the decoder allocates. Canonical padded base64 is required.
    let cap = remaining.min(MAX_ASSET_BYTES);
    if data.is_empty() || data.len() > cap.div_ceil(3) * 4 {
        return Err("audio asset exceeds preview byte limit or is empty".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| "invalid embedded audio base64".to_string())?;
    if bytes.is_empty() || bytes.len() > cap {
        return Err("audio asset exceeds preview byte limit or is empty".into());
    }
    Ok(bytes)
}

fn valid_cue(cue: &AudioCue) -> bool {
    cue.time.is_finite()
        && cue.time >= 0.0
        && cue.offset.is_finite()
        && cue.offset >= 0.0
        && cue.volume.is_finite()
        && (0.0..=1.0).contains(&cue.volume)
        && cue.duration.is_none_or(|d| d.is_finite() && d > 0.0)
        && (cue.offset + MAX_PREVIEW_SECONDS).is_finite()
}

fn new_cache() -> Result<Cache, String> {
    for _ in 0..16 {
        let directory = std::env::temp_dir().join(format!(
            "splinely-audio-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&directory) {
            Ok(()) => {
                return Ok(Cache {
                    directory,
                    cues: Vec::new(),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("cannot create audio preview cache: {e}")),
        }
    }
    Err("cannot create unique audio preview cache".into())
}

fn prepare(project: &Project) -> Result<Cache, String> {
    let cue_count = project
        .clips
        .iter()
        .try_fold(project.audio_cues.len(), |n, c| {
            n.checked_add(c.audio_cues.len())
        });
    if project.audio_assets.len() > MAX_ASSETS || cue_count.is_none_or(|n| n > MAX_CUES) {
        return Err("audio preview asset/cue count limit exceeded".into());
    }
    let mut cache = new_cache()?;
    let mut assets = Vec::new();
    let mut total = 0;
    for (index, asset) in project.audio_assets.iter().enumerate() {
        if assets.iter().any(|(id, _, _)| *id == &asset.id) {
            return Err("duplicate audio asset ID".into());
        }
        let format = format_for(&asset.mime_type)?;
        let bytes = decode(&asset.data_base64, MAX_CACHE_BYTES - total)?;
        total += bytes.len();
        // Never use document IDs/names as filesystem paths.
        let path = cache.directory.join(format!("{index}.audio"));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file: File = options.open(&path).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        assets.push((&asset.id, path, format));
    }
    let mut ids = HashSet::new();
    let cues = project
        .audio_cues
        .iter()
        .map(|cue| (None, f64::INFINITY, cue))
        .chain(project.clips.iter().flat_map(|clip| {
            clip.audio_cues
                .iter()
                .map(move |cue| (Some(clip.id.clone()), clip.duration, cue))
        }));
    for (scope, end, cue) in cues {
        if !valid_cue(cue) || !ids.insert(&cue.id) || end <= cue.time || end.is_nan() {
            return Err("invalid or duplicate audio preview cue".into());
        }
        let (_, path, format) = assets
            .iter()
            .find(|(id, _, _)| **id == cue.asset_id)
            .ok_or("audio cue references missing asset")?;
        cache.cues.push(PreparedCue {
            path: path.clone(),
            scope,
            format,
            time: cue.time,
            offset: cue.offset,
            length: cue
                .duration
                .unwrap_or(MAX_PREVIEW_SECONDS)
                .min(MAX_PREVIEW_SECONDS)
                .min(end - cue.time),
            volume: cue.volume,
        });
    }
    Ok(cache)
}

trait Process: Send {
    fn poll(&mut self) -> Result<Option<bool>, String>;
    fn terminate(&mut self);
}
// Guard also handles thread creation failure: dropping a captured closure must
// not orphan the already spawned child.
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn drain_errors(mut input: impl Read) -> String {
    let mut retained = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let count = n.min(4096 - retained.len());
                retained.extend_from_slice(&buffer[..count]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&retained).trim().to_string()
}

struct NativeProcess {
    cancel: mpsc::Sender<()>,
    result: mpsc::Receiver<Result<bool, String>>,
    supervisor: Option<JoinHandle<()>>,
}
impl NativeProcess {
    fn new(child: Child, timeout: Duration) -> Result<Self, String> {
        let mut guard = ChildGuard(child);
        let (cancel, cancelled) = mpsc::channel();
        let (finished, result) = mpsc::channel();
        let supervisor = thread::Builder::new()
            .name("splinely-audio-watchdog".into())
            .spawn(move || {
                let stderr = guard.0.stderr.take();
                let reader = match thread::Builder::new()
                    .name("splinely-audio-errors".into())
                    .spawn(move || stderr.map(drain_errors).unwrap_or_default())
                {
                    Ok(reader) => reader,
                    Err(e) => {
                        let _ = finished.send(Err(format!("audio error reader unavailable: {e}")));
                        return;
                    }
                };
                let deadline = Instant::now() + timeout;
                let outcome = loop {
                    match guard.0.try_wait() {
                        Ok(Some(status)) => break Ok(status.success()),
                        Err(e) => break Err(e.to_string()),
                        Ok(None) => {}
                    }
                    if Instant::now() >= deadline {
                        break Err("audio preview watchdog timeout".into());
                    }
                    match cancelled.recv_timeout(Duration::from_millis(10)) {
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        _ => break Ok(true),
                    }
                };
                // Close the pipe before joining its reader. Termination/reaping
                // occurs even without another GUI update.
                drop(guard);
                let errors = reader.join().unwrap_or_default();
                let outcome = if errors.is_empty() {
                    outcome
                } else {
                    Err(errors)
                };
                let _ = finished.send(outcome);
            })
            .map_err(|e| format!("audio supervisor unavailable: {e}"))?;
        Ok(Self {
            cancel,
            result,
            supervisor: Some(supervisor),
        })
    }
}
impl Process for NativeProcess {
    fn poll(&mut self) -> Result<Option<bool>, String> {
        match self.result.try_recv() {
            Ok(result) => result.map(Some),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err("audio supervisor stopped".into()),
        }
    }
    fn terminate(&mut self) {
        let _ = self.cancel.send(());
        if let Some(supervisor) = self.supervisor.take() {
            let _ = supervisor.join();
        }
    }
}
impl Drop for NativeProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

trait Player: Send {
    fn start(
        &mut self,
        cue: &PreparedCue,
        offset: f64,
        length: f64,
    ) -> Result<Box<dyn Process>, String>;
}
struct Ffplay;
impl Ffplay {
    fn command(cue: &PreparedCue, offset: f64, length: f64) -> Command {
        let mut command = Command::new("ffplay");
        command
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nodisp",
                "-autoexit",
                "-vn",
                "-sn",
                "-max_alloc",
                "33554432",
                "-threads",
                "1",
                "-filter_threads",
                "1",
                "-protocol_whitelist",
                "file",
                "-f",
                cue.format,
                "-ss",
            ])
            .arg(format!("{offset:.9}"))
            .arg("-t")
            .arg(format!("{length:.9}"))
            .arg("-volume")
            .arg(format!("{:.0}", cue.volume * 100.0))
            .arg("-i")
            .arg(&cue.path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // The editor has no console: do not flash one for the console-subsystem ffplay.
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        command
    }
}
impl Player for Ffplay {
    fn start(
        &mut self,
        cue: &PreparedCue,
        offset: f64,
        length: f64,
    ) -> Result<Box<dyn Process>, String> {
        let child = Self::command(cue, offset, length)
            .spawn()
            .map_err(|e| format!("native audio unavailable (install ffplay): {e}"))?;
        NativeProcess::new(child, Duration::from_secs_f64(length + 2.0))
            .map(|process| Box::new(process) as Box<dyn Process>)
    }
}
struct Voice {
    cue_index: usize,
    process: Box<dyn Process>,
    deadline: Instant,
    timeline_end: f64,
}
impl Drop for Voice {
    fn drop(&mut self) {
        self.process.terminate();
    }
}

/// A best-effort global and clip-local timeline preview. All errors are returned or queued,
/// never panicked. A failed spawn disables playback until set_project/retry.
/// Stderr is continuously drained with only 4 KiB retained per voice. Worker
/// threads supervise native processes even if GUI updates are suspended.
pub struct AudioPreview {
    cache: Option<Cache>,
    player: Box<dyn Player>,
    voices: Vec<Voice>,
    started: HashSet<usize>,
    clocks: std::collections::BTreeMap<Option<String>, (f64, bool)>,
    diagnostics: Vec<String>,
    playing: bool,
    position: f64,
    unavailable: bool,
    editor_tick: Option<Instant>,
}
impl Default for AudioPreview {
    fn default() -> Self {
        Self::new()
    }
}
impl AudioPreview {
    pub fn new() -> Self {
        Self {
            cache: None,
            player: Box::new(Ffplay),
            voices: Vec::new(),
            started: HashSet::new(),
            clocks: Default::default(),
            diagnostics: Vec::new(),
            playing: false,
            position: 0.0,
            unavailable: false,
            editor_tick: None,
        }
    }

    /// Stops old playback first. Failed preparation leaves preview empty (no
    /// stale project audio); caller can continue visual transport unchanged.
    pub fn set_project(&mut self, project: &Project) -> Result<(), String> {
        self.stop();
        self.cache = None;
        self.unavailable = false;
        self.diagnostics.clear();
        self.cache = Some(prepare(project)?);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.voices.clear();
        self.started.clear();
        self.clocks.clear();
        self.editor_tick = None;
        self.playing = false;
        self.position = 0.0;
    }

    /// Explicit discontinuity: recreate overlapping cues only if playing.
    /// Paused scrubbing never spawns a player.
    pub fn seek(&mut self, position: f64, playing: bool) {
        self.stop();
        self.update(position, playing);
    }

    /// Allow another spawn attempt after installing a player/fixing a device.
    pub fn retry(&mut self) {
        let position = self.position;
        let playing = self.playing;
        self.unavailable = false;
        self.seek(position, playing);
    }

    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        std::mem::take(&mut self.diagnostics)
    }
    fn report(&mut self, message: impl Into<String>) {
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            self.diagnostics.push(message.into());
        }
    }

    pub fn update(&mut self, position: f64, playing: bool) {
        self.update_clips(position, playing, &[]);
    }

    /// Synchronize against the editor's last transport tick (not wall time at
    /// this call). This detects forward seeks and complete loops even when the
    /// resulting local phase is equal to or greater than the previous phase.
    pub fn update_editor(
        &mut self,
        tick: Instant,
        position: f64,
        playing: bool,
        clips: &[(String, f64, bool, f64, bool)],
    ) {
        if let Some(previous) = self.editor_tick {
            let dt = tick.saturating_duration_since(previous).as_secs_f64();
            let mut reset = HashSet::new();
            for (scope, (old, was_playing)) in &self.clocks {
                let (next, duration, looping) = match scope {
                    None => (position, f64::INFINITY, false),
                    Some(id) => match clips.iter().find(|(key, ..)| key == id) {
                        Some((_, time, _, duration, looping)) => (*time, *duration, *looping),
                        None => {
                            reset.insert(scope.clone());
                            continue;
                        }
                    },
                };
                let advanced = old + if *was_playing { dt } else { 0.0 };
                let expected = if looping {
                    advanced % duration
                } else {
                    advanced.min(duration)
                };
                if (next - expected).abs() > 1e-7 || (looping && advanced >= duration) {
                    reset.insert(scope.clone());
                }
            }
            let indices: HashSet<usize> = self
                .cache
                .as_ref()
                .map(|cache| {
                    cache
                        .cues
                        .iter()
                        .enumerate()
                        .filter_map(|(i, cue)| reset.contains(&cue.scope).then_some(i))
                        .collect()
                })
                .unwrap_or_default();
            self.voices
                .retain(|voice| !indices.contains(&voice.cue_index));
            self.started.retain(|i| !indices.contains(i));
        }
        self.editor_tick = Some(tick);
        let clocks = clips
            .iter()
            .map(|(id, time, playing, ..)| (id.clone(), *time, *playing))
            .collect::<Vec<_>>();
        self.update_clips(position, playing, &clocks);
    }

    /// Clip clocks come from the same editor preview context as the visual pose.
    /// Weights deliberately do not affect audio. Missing/paused clocks stop
    /// their voices; resume/backward wraps reconstruct only overlapping cues.
    pub fn update_clips(&mut self, position: f64, playing: bool, clips: &[(String, f64, bool)]) {
        if !position.is_finite() || position < 0.0 {
            self.stop();
            self.report("invalid audio transport position");
            return;
        }
        let mut clocks = std::collections::BTreeMap::new();
        clocks.insert(None, (position, playing));
        for (id, time, playing) in clips {
            if !time.is_finite() || *time < 0.0 {
                self.stop();
                self.report("invalid clip audio transport position");
                return;
            }
            clocks.insert(Some(id.clone()), (*time, *playing));
        }
        let reset: HashSet<usize> = self
            .cache
            .as_ref()
            .map(|cache| {
                cache
                    .cues
                    .iter()
                    .enumerate()
                    .filter_map(|(i, cue)| {
                        let next = clocks.get(&cue.scope);
                        let old = self.clocks.get(&cue.scope);
                        (next.is_none_or(|(_, playing)| !playing)
                            || old.is_none_or(|(_, playing)| !playing)
                            || matches!((next, old), (Some((n, _)), Some((o, _))) if n < o))
                        .then_some(i)
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.voices
            .retain(|voice| !reset.contains(&voice.cue_index));
        self.started.retain(|i| !reset.contains(i));
        self.clocks = clocks;
        self.position = position;
        self.playing = playing;
        let now = Instant::now();
        let mut index = 0;
        while index < self.voices.len() {
            let status = self.voices[index].process.poll();
            let cue = &self.cache.as_ref().unwrap().cues[self.voices[index].cue_index];
            let local = self
                .clocks
                .get(&cue.scope)
                .map_or(f64::INFINITY, |(t, _)| *t);
            let expired =
                now >= self.voices[index].deadline || local >= self.voices[index].timeline_end;
            if expired || !matches!(status, Ok(None)) {
                self.voices.swap_remove(index);
                match status {
                    Ok(Some(false)) => self.report("native audio decoder/output failed"),
                    Err(e) => self.report(format!("native audio process failed: {e}")),
                    _ => {}
                }
            } else {
                index += 1;
            }
        }
        if self.unavailable {
            return;
        }
        let count = self.cache.as_ref().map_or(0, |c| c.cues.len());
        for index in 0..count {
            let cue = &self.cache.as_ref().unwrap().cues[index];
            let Some(&(position, true)) = self.clocks.get(&cue.scope) else {
                continue;
            };
            if position < cue.time || !self.started.insert(index) {
                continue;
            }
            let elapsed = position - cue.time;
            if elapsed >= cue.length || cue.volume == 0.0 {
                continue;
            }
            if self.voices.len() >= MAX_VOICES {
                self.report("native audio voice limit reached; cue skipped");
                continue;
            }
            let length = cue.length - elapsed;
            match self.player.start(cue, cue.offset + elapsed, length) {
                Ok(process) => self.voices.push(Voice {
                    cue_index: index,
                    process,
                    // Include startup allowance, but never run indefinitely.
                    deadline: now + Duration::from_secs_f64(length + 2.0),
                    timeline_end: cue.time + cue.length,
                }),
                Err(e) => {
                    self.report(e);
                    self.unavailable = true;
                    break;
                }
            }
        }
    }
}
impl Drop for AudioPreview {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use splinely_document::AudioAsset;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Calls {
        starts: Vec<(f64, f64)>,
        stops: usize,
        fail: bool,
        exited: bool,
    }
    struct FakePlayer(Arc<Mutex<Calls>>);
    struct FakeProcess(Arc<Mutex<Calls>>);
    impl Player for FakePlayer {
        fn start(
            &mut self,
            _: &PreparedCue,
            offset: f64,
            length: f64,
        ) -> Result<Box<dyn Process>, String> {
            let mut calls = self.0.lock().unwrap();
            calls.starts.push((offset, length));
            if calls.fail {
                return Err("missing player".into());
            }
            Ok(Box::new(FakeProcess(self.0.clone())))
        }
    }
    impl Process for FakeProcess {
        fn poll(&mut self) -> Result<Option<bool>, String> {
            Ok(self.0.lock().unwrap().exited.then_some(false))
        }
        fn terminate(&mut self) {
            self.0.lock().unwrap().stops += 1;
        }
    }
    fn project() -> Project {
        let mut p = Project::default();
        p.audio_assets.push(AudioAsset {
            id: "sound".into(),
            name: "../../untrusted".into(),
            mime_type: "audio/wav".into(),
            data_base64: base64::engine::general_purpose::STANDARD.encode(b"fake decoder input"),
        });
        p.audio_cues.push(AudioCue {
            id: "cue".into(),
            asset_id: "sound".into(),
            time: 2.0,
            offset: 1.0,
            duration: Some(4.0),
            volume: 0.5,
        });
        p
    }
    fn controller() -> (AudioPreview, Arc<Mutex<Calls>>) {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let mut preview = AudioPreview::new();
        preview.player = Box::new(FakePlayer(calls.clone()));
        preview.set_project(&project()).unwrap();
        (preview, calls)
    }

    #[test]
    fn clip_clocks_loop_pause_resume_scope_switch_and_end_truncation() {
        let (mut a, calls) = controller();
        let mut p = project();
        let cue = p.audio_cues.remove(0);
        p.clips.push(
            serde_json::from_value(serde_json::json!({
                "id":"clip", "duration":4, "looping":true, "audioCues":[cue]
            }))
            .unwrap(),
        );
        a.set_project(&p).unwrap();
        let clock = |time, playing| vec![("clip".to_string(), time, playing)];
        a.update_clips(10., false, &clock(2., true));
        assert_eq!(calls.lock().unwrap().starts, [(1., 2.)]);
        a.update_clips(11., false, &clock(3., false));
        assert_eq!(a.active_voices(), 0);
        a.update_clips(11., false, &clock(3., true));
        assert_eq!(calls.lock().unwrap().starts[1], (2., 1.));
        a.update_clips(12., false, &clock(0., true));
        assert_eq!(a.active_voices(), 0);
        a.update_clips(14., false, &clock(2., true));
        assert_eq!(calls.lock().unwrap().starts.len(), 3);
        a.update_clips(15., false, &[]);
        assert_eq!(a.active_voices(), 0);
        a.update_clips(16., false, &clock(4., true));
        assert_eq!(calls.lock().unwrap().starts.len(), 3);
    }

    #[test]
    fn editor_tick_detects_forward_seek_and_skipped_complete_loop_per_scope() {
        let (mut a, calls) = controller();
        let mut p = project();
        let mut cue = p.audio_cues[0].clone();
        cue.id = "local".into();
        cue.time = 0.;
        p.clips.push(
            serde_json::from_value(serde_json::json!({
                "id":"clip", "duration":2, "looping":true, "audioCues":[cue]
            }))
            .unwrap(),
        );
        a.set_project(&p).unwrap();
        let tick = Instant::now();
        let clock = |time| vec![("clip".to_string(), time, true, 2., true)];
        a.update_editor(tick, 2., true, &clock(0.5));
        a.update_editor(tick + Duration::from_millis(100), 2.1, true, &clock(1.));
        // Forward local seek, but the unrelated global voice keeps playing.
        assert_eq!(
            calls.lock().unwrap().starts,
            [(1., 4.), (1.5, 1.5), (2., 1.)]
        );
        assert_eq!(calls.lock().unwrap().stops, 1);
        a.update_editor(tick + Duration::from_millis(2200), 4.2, true, &clock(1.1));
        // A whole loop elapsed even though local phase increased.
        assert_eq!(calls.lock().unwrap().starts[3], (2.1, 0.8999999999999999));
        assert_eq!(calls.lock().unwrap().stops, 2);
        assert_eq!(a.active_voices(), 2);
    }

    #[test]
    fn clip_and_global_voices_are_independent_and_limits_include_all_scopes() {
        let (mut a, calls) = controller();
        let mut p = project();
        let mut cue = p.audio_cues[0].clone();
        cue.id = "local".into();
        p.clips.push(
            serde_json::from_value(serde_json::json!({
                "id":"clip", "duration":8, "audioCues":[cue]
            }))
            .unwrap(),
        );
        a.set_project(&p).unwrap();
        a.update_clips(2., true, &[("clip".into(), 3., true)]);
        assert_eq!(calls.lock().unwrap().starts, [(1., 4.), (2., 3.)]);
        a.update_clips(3., true, &[("clip".into(), 3., false)]);
        assert_eq!(a.active_voices(), 1);
        p.clips[0].audio_cues = (0..MAX_CUES)
            .map(|i| {
                let mut cue = p.clips[0].audio_cues[0].clone();
                cue.id = format!("local-{i}");
                cue
            })
            .collect();
        assert!(a.set_project(&p).is_err());
        assert!(a.cache.is_none());
    }

    #[test]
    fn transport_pause_resume_seek_and_completion() {
        let (mut a, calls) = controller();
        a.update(0.0, true);
        assert_eq!(a.active_voices(), 0);
        a.update(2.0, true);
        a.update(3.0, true);
        assert_eq!(calls.lock().unwrap().starts, vec![(1.0, 4.0)]);
        a.update(3.0, false);
        assert_eq!(calls.lock().unwrap().stops, 1);
        a.update(3.0, true);
        assert_eq!(calls.lock().unwrap().starts[1], (2.0, 3.0));
        a.seek(4.0, true);
        assert_eq!(calls.lock().unwrap().starts[2], (3.0, 2.0));
        a.seek(5.0, false);
        assert_eq!(a.active_voices(), 0);
        a.update(7.0, true);
        assert_eq!(calls.lock().unwrap().starts.len(), 3);
    }

    #[test]
    fn backward_wrap_and_drop_stop_children_and_remove_cache() {
        let (mut a, calls) = controller();
        let dir = a.cache.as_ref().unwrap().directory.clone();
        assert!(dir.join("0.audio").exists());
        a.update(3.0, true);
        a.update(2.0, true);
        assert_eq!(calls.lock().unwrap().stops, 1);
        drop(a);
        assert_eq!(calls.lock().unwrap().stops, 2);
        assert!(!dir.exists());
    }

    #[test]
    fn limits_and_invalid_inputs_fail_closed() {
        assert!(decode("!!!!", MAX_ASSET_BYTES).is_err());
        assert!(decode("", MAX_ASSET_BYTES).is_err());
        assert!(decode("AAAA", 2).is_err());
        assert!(decode("AAAA", 3).is_ok());
        assert!(decode(
            &"A".repeat(MAX_ASSET_BYTES.div_ceil(3) * 4 + 1),
            MAX_ASSET_BYTES
        )
        .is_err());
        let (mut a, _) = controller();
        let mut p = project();
        p.audio_cues[0].offset = f64::NAN;
        assert!(a.set_project(&p).is_err());
        assert!(a.cache.is_none());
        p = project();
        p.audio_assets[0].mime_type = "text/plain".into();
        assert!(a.set_project(&p).is_err());
        p = project();
        p.audio_cues[0].asset_id = "absent".into();
        assert!(a.set_project(&p).is_err());
        p = project();
        p.audio_cues = vec![p.audio_cues[0].clone(); MAX_CUES + 1];
        assert!(a.set_project(&p).is_err());
    }

    #[test]
    fn concurrency_failures_and_retry_are_bounded() {
        let (mut a, calls) = controller();
        let mut p = project();
        p.audio_cues = (0..MAX_VOICES + 3)
            .map(|i| {
                let mut c = p.audio_cues[0].clone();
                c.id = i.to_string();
                c
            })
            .collect();
        a.set_project(&p).unwrap();
        a.update(2.0, true);
        assert_eq!(a.active_voices(), MAX_VOICES);
        assert_eq!(a.take_diagnostics().len(), 3);
        a.update(2.1, true);
        assert_eq!(calls.lock().unwrap().starts.len(), MAX_VOICES);
        a.stop();
        calls.lock().unwrap().fail = true;
        a.update(2.0, true);
        a.update(2.1, true);
        assert_eq!(a.take_diagnostics(), vec!["missing player"]);
        assert_eq!(calls.lock().unwrap().starts.len(), MAX_VOICES + 1);
        calls.lock().unwrap().fail = false;
        a.retry();
        assert_eq!(a.active_voices(), MAX_VOICES);
    }

    #[test]
    fn decoder_exit_invalid_time_and_watchdog_are_nonfatal() {
        let (mut a, calls) = controller();
        a.update(2.0, true);
        calls.lock().unwrap().exited = true;
        a.update(2.1, true);
        assert_eq!(a.active_voices(), 0);
        assert_eq!(
            a.take_diagnostics(),
            vec!["native audio decoder/output failed"]
        );
        calls.lock().unwrap().exited = false;
        a.seek(2.0, true);
        a.voices[0].deadline = Instant::now();
        a.update(2.2, true);
        assert_eq!(a.active_voices(), 0);
        a.update(f64::INFINITY, true);
        assert!(!a.playing);
        assert_eq!(
            a.take_diagnostics(),
            vec!["invalid audio transport position"]
        );
    }

    #[test]
    fn stderr_is_drained_with_bounded_retention() {
        let errors = drain_errors(std::io::Cursor::new(vec![b'x'; 1024 * 1024]));
        assert_eq!(errors.len(), 4096);
    }

    #[test]
    #[cfg(unix)]
    fn native_watchdog_works_without_transport_updates() {
        let child = Command::new("/bin/sh")
            .args(["-c", "exec sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut process = NativeProcess::new(child, Duration::from_millis(40)).unwrap();
        // No controller.update calls: the worker kills/reaps independently.
        let result = process.result.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(result, Err("audio preview watchdog timeout".into()));
        process.terminate();
    }

    #[test]
    fn transport_end_stops_a_still_running_voice() {
        let (mut a, calls) = controller();
        a.update(2.0, true);
        a.update(6.0, true);
        assert_eq!(a.active_voices(), 0);
        assert_eq!(calls.lock().unwrap().stops, 1);
    }

    #[test]
    #[ignore = "requires installed ffplay with WAV decoder and SDL dummy audio"]
    fn installed_ffplay_decodes_embedded_wav_and_reports_bad_input() {
        let mut p = project();
        // 100 ms mono PCM WAV, generated without any fixture/dependency.
        let samples = 800u32;
        let data_size = samples * 2;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_size).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        wav.resize(44 + data_size as usize, 0);
        p.audio_assets[0].data_base64 = base64::engine::general_purpose::STANDARD.encode(wav);
        let cache = prepare(&p).unwrap();
        let child = Ffplay::command(&cache.cues[0], 0.02, 0.04)
            .env("SDL_AUDIODRIVER", "dummy")
            .spawn()
            .unwrap();
        let mut process = NativeProcess::new(child, Duration::from_secs(5)).unwrap();
        assert_eq!(
            process.result.recv_timeout(Duration::from_secs(6)).unwrap(),
            Ok(true)
        );
        process.terminate();
        let bad = prepare(&project()).unwrap();
        let child = Ffplay::command(&bad.cues[0], 0.0, 0.1)
            .env("SDL_AUDIODRIVER", "dummy")
            .spawn()
            .unwrap();
        let mut process = NativeProcess::new(child, Duration::from_secs(5)).unwrap();
        // ffplay can exit 0 on decode failure: stderr must still diagnose it.
        assert!(process
            .result
            .recv_timeout(Duration::from_secs(6))
            .unwrap()
            .is_err());
        process.terminate();
    }

    #[test]
    fn duration_is_capped_and_forward_seek_skips_finished_cues() {
        let (mut a, calls) = controller();
        let mut p = project();
        p.audio_cues[0].duration = None;
        a.set_project(&p).unwrap();
        a.update(2.0, true);
        assert_eq!(calls.lock().unwrap().starts[0], (1.0, MAX_PREVIEW_SECONDS));
        a.seek(400.0, true);
        assert_eq!(a.active_voices(), 0);
        assert_eq!(calls.lock().unwrap().starts.len(), 1);
    }
}
