//! Local text-to-speech for embedded audio (no network, no shell). Picks the
//! best installed engine – Piper (neural, with an explicit model), Flite's
//! `slt` voice for English, otherwise eSpeak NG with tuned rate/pitch – and
//! polishes the WAV (rumble/buzz filtering, silence trimming, fades and
//! loudness normalisation) so it sounds clearer and evenly loud.
use crate::DocumentError;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const MAX_SPEECH_TEXT_BYTES: usize = 4096;
const MAX_WAV_BYTES: u64 = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpeechEngine {
    /// Piper with a model, else Flite for English, else eSpeak NG.
    #[default]
    Auto,
    Espeak,
    Flite,
    Piper,
}

#[derive(Clone, Debug)]
pub struct SpeechRequest {
    pub text: String,
    /// BCP 47 language such as "de-DE" or "en-US".
    pub lang: String,
    /// Engine-specific voice (eSpeak "de+anika", Flite "slt"); default per language.
    pub voice: Option<String>,
    /// 1 = natural speed.
    pub rate: f64,
    /// 1 = natural pitch.
    pub pitch: f64,
    pub engine: SpeechEngine,
    /// Absolute path of the engine executable (default: found on PATH).
    pub engine_path: Option<PathBuf>,
    /// Piper voice model (.onnx).
    pub piper_model: Option<PathBuf>,
    /// Apply filtering, trimming and normalisation.
    pub polish: bool,
}

impl Default for SpeechRequest {
    fn default() -> Self {
        Self {
            text: String::new(),
            lang: "en-US".into(),
            voice: None,
            rate: 1.,
            pitch: 1.,
            engine: SpeechEngine::Auto,
            engine_path: None,
            piper_model: None,
            polish: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Speech {
    pub wav: Vec<u8>,
    /// e.g. "flite slt" or "espeak-ng de".
    pub engine: String,
    pub seconds: f64,
}

fn err(message: impl Into<String>) -> DocumentError {
    DocumentError(message.into())
}

/// Find an executable on PATH.
pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// Installed engines as (name, path), best first.
pub fn available_engines() -> Vec<(&'static str, PathBuf)> {
    ["piper", "flite", "espeak-ng", "espeak"]
        .into_iter()
        .filter_map(|n| which(n).map(|p| (n, p)))
        .collect()
}

fn is_english(lang: &str) -> bool {
    lang.to_lowercase().starts_with("en")
}

/// eSpeak voice for a BCP 47 language ("de-DE" → "de", "en-US" → "en-us").
fn espeak_voice(lang: &str) -> String {
    let lower = lang.to_lowercase();
    match lower.as_str() {
        l if l.starts_with("en-us") => "en-us".into(),
        l if l.starts_with("en") => "en-gb".into(),
        l if l.starts_with("pt-br") => "pt-br".into(),
        l => l.split(['-', '_']).next().unwrap_or("en").into(),
    }
}

fn valid_voice(voice: &str) -> bool {
    !voice.is_empty()
        && voice.len() <= 128
        && voice
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_+".contains(&b))
}

/// Pauses after sentences and clauses help eSpeak/Flite phrasing.
fn prepare_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        out.push_str(line);
        // Line breaks are sentence ends for the listener.
        if !line.ends_with(['.', '!', '?', ':', ';', ',']) {
            out.push('.');
        }
        out.push(' ');
    }
    out.trim_end().to_owned()
}

// Reap even on polling/filesystem errors.
struct EngineChild(Child);
impl Drop for EngineChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir() -> Result<TempDir, DocumentError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "splinely-speech-{}-{nanos}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).map_err(|e| err(format!("speech temp dir: {e}")))?;
    Ok(TempDir(dir))
}

/// Run an engine with a deadline and read the WAV it wrote.
fn run_engine(
    mut command: Command,
    stdin_text: Option<&str>,
    wav: &Path,
    name: &str,
) -> Result<Vec<u8>, DocumentError> {
    command
        .stdin(if stdin_text.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = command
        .spawn()
        .map_err(|e| err(format!("Cannot start local {name} engine: {e}")))?;
    let mut child = EngineChild(child);
    if let (Some(text), Some(mut stdin)) = (stdin_text, child.0.stdin.take()) {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| err(format!("{name}: {e}")))?;
    }
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if std::fs::metadata(wav).is_ok_and(|m| m.len() > MAX_WAV_BYTES) {
            return Err(err("Generated WAV exceeds 8 MiB"));
        }
        if let Some(status) = child
            .0
            .try_wait()
            .map_err(|e| err(format!("{name}: {e}")))?
        {
            if !status.success() {
                return Err(err(format!("Local {name} engine failed: {status}")));
            }
            break;
        }
        if Instant::now() >= deadline {
            return Err(err(format!(
                "Local {name} engine timed out after 30 seconds"
            )));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    // Never open a FIFO or device a misbehaving engine may have left.
    let metadata =
        std::fs::symlink_metadata(wav).map_err(|e| err(format!("{name} wrote no WAV: {e}")))?;
    if !metadata.file_type().is_file() {
        return Err(err(
            "Generated WAV must be a regular file, not a symlink or device",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(wav)
        .map_err(|e| err(e.to_string()))?
        .take(MAX_WAV_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| err(e.to_string()))?;
    if bytes.len() as u64 > MAX_WAV_BYTES {
        return Err(err("Generated WAV exceeds 8 MiB"));
    }
    if !valid_wav(&bytes) {
        return Err(err(format!(
            "Local {name} engine did not generate a PCM WAV file"
        )));
    }
    Ok(bytes)
}

/// Synthesize speech with a local engine.
pub fn synthesize(request: &SpeechRequest) -> Result<Speech, DocumentError> {
    let text = request.text.trim();
    if text.is_empty() || request.text.len() > MAX_SPEECH_TEXT_BYTES || text.contains('\0') {
        return Err(err(
            "Text must be nonempty UTF-8, without NUL, at most 4096 bytes",
        ));
    }
    if !(request.rate.is_finite() && (0.3..=3.).contains(&request.rate)) {
        return Err(err("Rate must be within 0.3..3"));
    }
    if !(request.pitch.is_finite() && (0.3..=2.).contains(&request.pitch)) {
        return Err(err("Pitch must be within 0.3..2"));
    }
    if let Some(voice) = &request.voice {
        if !valid_voice(voice) {
            return Err(err(
                "Voice must be an engine identifier (letters, digits, -, _, +), at most 128 bytes",
            ));
        }
    }
    if request
        .engine_path
        .as_ref()
        .is_some_and(|p| !p.is_absolute())
    {
        return Err(err("The engine path must be absolute"));
    }
    let engine = match request.engine {
        SpeechEngine::Auto if request.engine_path.is_some() && request.piper_model.is_none() => {
            SpeechEngine::Espeak
        }
        SpeechEngine::Auto if request.piper_model.is_some() => SpeechEngine::Piper,
        SpeechEngine::Auto
            if is_english(&request.lang) && request.voice.is_none() && which("flite").is_some() =>
        {
            SpeechEngine::Flite
        }
        SpeechEngine::Auto => SpeechEngine::Espeak,
        other => other,
    };
    let dir = temp_dir()?;
    let source = dir.0.join("text.txt");
    let wav = dir.0.join("speech.wav");
    let prepared = prepare_text(text);
    std::fs::write(&source, &prepared).map_err(|e| err(e.to_string()))?;
    let locate = |names: &[&str]| -> Result<PathBuf, DocumentError> {
        request
            .engine_path
            .clone()
            .or_else(|| names.iter().find_map(|n| which(n)))
            .ok_or_else(|| {
                err(format!(
                    "No local {} found; install it or pass an absolute engine path",
                    names[0]
                ))
            })
    };
    let (bytes, label) = match engine {
        SpeechEngine::Espeak => {
            let path = locate(&["espeak-ng", "espeak"])?;
            let voice = request
                .voice
                .clone()
                .unwrap_or_else(|| espeak_voice(&request.lang));
            // eSpeak's default 175 wpm sounds rushed; 150 wpm with a small
            // word gap and a slightly lower pitch is clearer.
            let speed = (150. * request.rate).round().clamp(80., 450.) as i64;
            let pitch = (45. * request.pitch).round().clamp(0., 99.) as i64;
            let mut command = Command::new(&path);
            command
                .args([
                    "-b",
                    "1",
                    "-v",
                    &voice,
                    "-s",
                    &speed.to_string(),
                    "-p",
                    &pitch.to_string(),
                    "-g",
                    "1",
                    "-w",
                ])
                .arg(&wav)
                .arg("-f")
                .arg(&source);
            (
                run_engine(command, None, &wav, "eSpeak")?,
                format!("espeak-ng {voice}"),
            )
        }
        SpeechEngine::Flite => {
            let path = locate(&["flite"])?;
            let voice = request.voice.clone().unwrap_or_else(|| "slt".into());
            let mut command = Command::new(&path);
            command
                .args(["-voice", &voice, "--setf"])
                .arg(format!("duration_stretch={:.3}", 1.05 / request.rate));
            if (request.pitch - 1.).abs() > 1e-6 {
                command
                    .arg("--setf")
                    .arg(format!("int_f0_target_mean={:.1}", 170. * request.pitch));
            }
            command.arg("-f").arg(&source).arg("-o").arg(&wav);
            (
                run_engine(command, None, &wav, "Flite")?,
                format!("flite {voice}"),
            )
        }
        SpeechEngine::Piper => {
            let path = locate(&["piper"])?;
            let model = request.piper_model.clone().ok_or_else(|| {
                err("Piper needs a voice model (.onnx), e.g. de_DE-thorsten-high.onnx")
            })?;
            if !model.is_file() {
                return Err(err(format!("Piper model not found: {}", model.display())));
            }
            let mut command = Command::new(&path);
            command
                .arg("--model")
                .arg(&model)
                .arg("--output_file")
                .arg(&wav)
                .arg("--length_scale")
                .arg(format!("{:.3}", 1. / request.rate));
            let name = model
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            (
                run_engine(command, Some(&prepared), &wav, "Piper")?,
                format!("piper {name}"),
            )
        }
        SpeechEngine::Auto => unreachable!("resolved above"),
    };
    let bytes = if request.polish {
        polish_wav(&bytes).unwrap_or(bytes)
    } else {
        bytes
    };
    let seconds = wav_seconds(&bytes).unwrap_or(0.);
    Ok(Speech {
        wav: bytes,
        engine: label,
        seconds,
    })
}

/// PCM RIFF/WAVE with a non-empty data chunk and consistent sizes.
pub fn valid_wav(bytes: &[u8]) -> bool {
    parse_wav(bytes).is_some_and(|w| !w.data.is_empty())
}

struct Wav<'a> {
    channels: u16,
    rate: u32,
    bits: u16,
    data: &'a [u8],
}

fn parse_wav(bytes: &[u8]) -> Option<Wav<'_>> {
    if bytes.len() < 44 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
    let u16_at = |i: usize| u16::from_le_bytes(bytes[i..i + 2].try_into().unwrap());
    // Some engines (Flite, streaming writers) leave the RIFF size imprecise;
    // accept it as long as every chunk fits.
    let mut pos = 12;
    let mut format = None;
    let mut data = None;
    while pos + 8 <= bytes.len() {
        let size = u32_at(pos + 4);
        let start = pos + 8;
        let size = size.min(bytes.len() - start);
        match &bytes[pos..pos + 4] {
            b"fmt " if size >= 16 => {
                let (tag, channels, rate, bits) = (
                    u16_at(start),
                    u16_at(start + 2),
                    u32_at(start + 4) as u32,
                    u16_at(start + 14),
                );
                if tag == 1 && channels > 0 && rate > 0 && bits > 0 {
                    format = Some((channels, rate, bits));
                }
            }
            b"data" => data = Some(&bytes[start..start + size]),
            _ => {}
        }
        pos = start + size + (size % 2);
    }
    let (channels, rate, bits) = format?;
    Some(Wav {
        channels,
        rate,
        bits,
        data: data?,
    })
}

fn wav_seconds(bytes: &[u8]) -> Option<f64> {
    let w = parse_wav(bytes)?;
    let frame = (w.bits as usize / 8) * w.channels as usize;
    Some(w.data.len() as f64 / frame.max(1) as f64 / w.rate as f64)
}

/// Mono 16-bit PCM WAV.
pub fn write_wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let mut out = Vec::with_capacity(44 + data_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1., 1.) * 32767.).round() as i16).to_le_bytes());
    }
    out
}

/// Second-order filter (RBJ cookbook), direct form I.
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl Biquad {
    fn new(kind: char, freq: f32, rate: f32, q: f32, gain_db: f32) -> Self {
        let w = 2. * std::f32::consts::PI * freq / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2. * q);
        let a_gain = 10f32.powf(gain_db / 40.);
        let (b, a0, a) = match kind {
            'h' => (
                [(1. + cos) / 2., -(1. + cos), (1. + cos) / 2.],
                1. + alpha,
                [-2. * cos, 1. - alpha],
            ),
            'l' => (
                [(1. - cos) / 2., 1. - cos, (1. - cos) / 2.],
                1. + alpha,
                [-2. * cos, 1. - alpha],
            ),
            // Peaking EQ.
            _ => (
                [1. + alpha * a_gain, -2. * cos, 1. - alpha * a_gain],
                1. + alpha / a_gain,
                [-2. * cos, 1. - alpha / a_gain],
            ),
        };
        Self {
            b: b.map(|v| v / a0),
            a: a.map(|v| v / a0),
            x: [0.; 2],
            y: [0.; 2],
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let out = self.b[0] * input + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [input, self.x[0]];
        self.y = [out, self.y[0]];
        out
    }
}

/// Clean up synthetic speech: remove rumble, soften the buzzy top end typical
/// of formant synthesis, add a little warmth, trim silence, fade the edges and
/// normalise loudness with a soft peak limit.
pub fn polish_wav(bytes: &[u8]) -> Option<Vec<u8>> {
    let wav = parse_wav(bytes)?;
    if wav.bits != 16 {
        return None;
    }
    let channels = wav.channels as usize;
    let mut samples: Vec<f32> = wav
        .data
        .chunks_exact(2 * channels)
        .map(|frame| {
            frame
                .as_chunks::<2>()
                .0
                .iter()
                .map(|s| i16::from_le_bytes(*s) as f32 / 32768.)
                .sum::<f32>()
                / channels as f32
        })
        .collect();
    if samples.is_empty() {
        return None;
    }
    let rate = wav.rate as f32;
    let nyquist = rate / 2.;
    let mut filters = vec![
        Biquad::new('h', 85., rate, 0.707, 0.),
        // Warmth around the voice's body, gentle cut of the harsh buzz band.
        Biquad::new('p', 220., rate, 0.9, 2.0),
        Biquad::new('p', (3200f32).min(nyquist * 0.7), rate, 1.2, -2.5),
        Biquad::new('l', (7000f32).min(nyquist * 0.85), rate, 0.707, 0.),
    ];
    for s in &mut samples {
        let mut v = *s;
        for f in &mut filters {
            v = f.process(v);
        }
        *s = v;
    }
    // Trim leading/trailing silence, keeping a short natural margin.
    let threshold = 10f32.powf(-42. / 20.);
    let first = samples.iter().position(|s| s.abs() > threshold)?;
    let last = samples.iter().rposition(|s| s.abs() > threshold)?;
    let start = first.saturating_sub((0.06 * rate) as usize);
    let end = (last + (0.18 * rate) as usize).min(samples.len());
    let mut samples = samples[start..end].to_vec();
    // Loudness: RMS of the voiced parts to about -14 dBFS (≈ -18 LUFS), peaks soft-limited.
    let voiced: Vec<f32> = samples
        .iter()
        .copied()
        .filter(|s| s.abs() > threshold * 4.)
        .collect();
    let rms = (voiced.iter().map(|s| s * s).sum::<f32>() / voiced.len().max(1) as f32).sqrt();
    let gain = if rms > 0. {
        (10f32.powf(-14. / 20.) / rms).min(16.)
    } else {
        1.
    };
    let ceiling = 10f32.powf(-1. / 20.);
    for s in &mut samples {
        let v = *s * gain;
        // Soft knee above 70 % of the ceiling.
        let knee = ceiling * 0.7;
        *s = if v.abs() <= knee {
            v
        } else {
            let over = (v.abs() - knee) / (ceiling - knee);
            v.signum() * (knee + (ceiling - knee) * over.tanh())
        };
    }
    // 10 ms fades against clicks.
    let fade = ((0.01 * rate) as usize).min(samples.len() / 2);
    let n = samples.len();
    for i in 0..fade {
        let g = i as f32 / fade as f32;
        samples[i] *= g;
        samples[n - 1 - i] *= g;
    }
    Some(write_wav(&samples, wav.rate))
}

/// Standard padded RFC 4648 base64.
pub fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(a >> 2) as usize] as char);
        out.push(TABLE[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_vectors() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode_base64(input.as_bytes()), expected);
        }
    }

    #[test]
    fn polishing_trims_normalises_and_keeps_a_valid_wav() {
        let rate = 16000;
        // 0.5 s silence, 0.5 s quiet 200 Hz tone, 0.5 s silence.
        let mut samples = vec![0f32; rate as usize / 2];
        samples.extend(
            (0..rate / 2)
                .map(|i| 0.05 * (i as f32 * 200. * std::f32::consts::TAU / rate as f32).sin()),
        );
        samples.extend(vec![0f32; rate as usize / 2]);
        let wav = write_wav(&samples, rate);
        assert!(valid_wav(&wav));
        let polished = polish_wav(&wav).unwrap();
        assert!(valid_wav(&polished));
        let seconds = wav_seconds(&polished).unwrap();
        assert!(seconds > 0.5 && seconds < 0.8, "{seconds}");
        let out = parse_wav(&polished).unwrap();
        let peak = out
            .data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|s| i16::from_le_bytes(*s).unsigned_abs())
            .max()
            .unwrap();
        assert!(
            peak > 3500,
            "louder after normalising (input peak 1638): {peak}"
        );
        // Silence alone cannot be polished.
        assert!(polish_wav(&write_wav(&[0.; 1000], rate)).is_none());
    }

    #[test]
    fn language_maps_to_engine_voices() {
        assert_eq!(espeak_voice("de-DE"), "de");
        assert_eq!(espeak_voice("en-US"), "en-us");
        assert_eq!(espeak_voice("en"), "en-gb");
        assert_eq!(prepare_text("Hallo\nWelt"), "Hallo. Welt.");
        assert!(synthesize(&SpeechRequest {
            text: " ".into(),
            ..Default::default()
        })
        .is_err());
        assert!(synthesize(&SpeechRequest {
            text: "Hi".into(),
            rate: 9.,
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn installed_engines_produce_speech() {
        for (lang, engine) in [
            ("de-DE", SpeechEngine::Espeak),
            ("en-US", SpeechEngine::Auto),
        ] {
            if which("espeak-ng").is_none() && which("espeak").is_none() {
                return;
            }
            let speech = synthesize(&SpeechRequest {
                text: "Hallo, das ist ein Test.".into(),
                lang: lang.into(),
                engine,
                ..Default::default()
            })
            .unwrap();
            assert!(valid_wav(&speech.wav), "{}", speech.engine);
            assert!(speech.seconds > 0.5, "{} {}", speech.engine, speech.seconds);
        }
    }
}
