//! Local speech synthesis (no shell, downloads or browser speech) via the
//! shared `splinely_document::tts` engines, embedded as a WAV audio asset + cue.
use crate::{failure, load, save_project, Result};
use splinely_document::tts::{encode_base64, synthesize, SpeechEngine, SpeechRequest};
use splinely_document::{AudioAsset, AudioCue};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Synth {
    Auto,
    Espeak,
    Flite,
    Piper,
}

impl From<Synth> for SpeechEngine {
    fn from(s: Synth) -> Self {
        match s {
            Synth::Auto => SpeechEngine::Auto,
            Synth::Espeak => SpeechEngine::Espeak,
            Synth::Flite => SpeechEngine::Flite,
            Synth::Piper => SpeechEngine::Piper,
        }
    }
}

pub fn run(
    input: &str,
    output: &str,
    asset_id: &str,
    time: f64,
    request: SpeechRequest,
) -> Result<Value> {
    let mut project = load(input)?;
    let text = &request.text;
    if text.trim().is_empty()
        || text.len() > splinely_document::tts::MAX_SPEECH_TEXT_BYTES
        || text.contains('\0')
    {
        return Err(failure(
            "invalid_speech",
            "Text must be nonempty UTF-8, without NUL, at most 4096 bytes",
        ));
    }
    if !time.is_finite() || time < 0.0 {
        return Err(failure(
            "invalid_speech",
            "Time must be finite and nonnegative",
        ));
    }
    if request
        .engine_path
        .as_ref()
        .is_some_and(|p| !p.is_absolute())
    {
        return Err(failure(
            "invalid_speech",
            "--engine requires an absolute local executable path",
        ));
    }
    if let Some(voice) = &request.voice {
        let ok = !voice.is_empty()
            && voice.len() <= 128
            && voice
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_+".contains(&b));
        if !ok {
            return Err(failure(
                "invalid_speech",
                "Voice must be an engine identifier (letters, digits, -, _, +), at most 128 bytes",
            ));
        }
    }
    project.audio_assets.push(AudioAsset {
        id: asset_id.into(),
        name: format!("Speech: {}", text.chars().take(40).collect::<String>()),
        mime_type: "audio/wav".into(),
        data_base64: "UklGRg==".into(), // Validate IDs/cue before running any executable.
    });
    project.audio_cues.push(AudioCue {
        id: format!("{asset_id}-cue"),
        asset_id: asset_id.into(),
        time,
        offset: 0.0,
        duration: None,
        volume: 1.0,
    });
    project
        .validate()
        .map_err(|e| failure("validation_error", e))?;
    let speech = synthesize(&request).map_err(|e| {
        let message = e.to_string();
        let input_problem = ["Text must", "Rate must", "Pitch must", "Voice must", "The engine path"]
            .iter()
            .any(|p| message.starts_with(p));
        // Unusable output (not a WAV, a FIFO/symlink, too large) vs. an engine that failed.
        let output_problem = ["Generated WAV", "wrote no WAV", "did not generate"]
            .iter()
            .any(|p| message.contains(p));
        let code = if input_problem {
            "invalid_speech"
        } else if output_problem {
            "speech_error"
        } else {
            "speech_engine_error"
        };
        let error = failure(code, message);
        if input_problem {
            error
        } else {
            error.hint("Installed engines: splinely-cli speech-engines. For natural voices install Piper and pass --piper-model VOICE.onnx.")
        }
    })?;
    project
        .audio_assets
        .last_mut()
        .expect("asset appended")
        .data_base64 = encode_base64(&speech.wav);
    let mut result = save_project(&project, output)?;
    if output != "-" {
        result["engine"] = json!(speech.engine);
        result["seconds"] = json!((speech.seconds * 100.).round() / 100.);
    }
    Ok(result)
}

/// Installed engines and which one `auto` picks per language.
pub fn engines() -> Result<Value> {
    let installed: Vec<Value> = splinely_document::tts::available_engines()
        .into_iter()
        .map(|(name, path)| json!({"name": name, "path": path}))
        .collect();
    let has = |n: &str| splinely_document::tts::which(n).is_some();
    let fallback = if has("espeak-ng") || has("espeak") {
        "espeak-ng"
    } else {
        "none"
    };
    Ok(json!({
        "ok": true,
        "installed": installed,
        "auto": {
            "english": if has("flite") { "flite slt" } else { fallback },
            "other": fallback,
            "withPiperModel": if has("piper") { "piper" } else { "piper (not installed)" },
        },
        "tips": [
            "Best quality: Piper neural voices (e.g. de_DE-thorsten-high, en_US-lessac-high) with --synth piper --piper-model FILE.onnx.",
            "eSpeak variants change the character: --voice de+anika, de+klatt, en-us+f3; Flite voices: slt, kal, awb, rms.",
            "--rate 0.9 slows speech down slightly, which often sounds more natural.",
        ],
    }))
}
