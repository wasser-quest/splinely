use splinely_document::{AudioAsset, AudioCue, Project};

fn project() -> Project {
    // Deserialize to exercise compatibility defaults without depending on blend-space fields.
    let mut p = Project::default();
    p.clips.push(
        serde_json::from_value(serde_json::json!({
            "id":"intro", "duration":2, "looping":true, "audioCues":[{
                "id":"hit", "assetId":"sound", "time":0.5, "offset":0.1,
                "duration":0.2, "volume":0.7
            }]
        }))
        .unwrap(),
    );
    p.audio_assets.push(AudioAsset {
        id: "sound".into(),
        name: "Sound".into(),
        mime_type: "audio/wav".into(),
        data_base64: "AQID".into(),
    });
    p
}
#[test]
fn clip_audio_cues_round_trip_and_old_clips_default_empty() {
    let p = project();
    p.validate().unwrap();
    let json = p.to_json().unwrap();
    assert!(json.contains("audioCues"));
    assert_eq!(Project::from_json(&json).unwrap(), p);
    let clip: splinely_document::Clip =
        serde_json::from_value(serde_json::json!({"id":"old","duration":1})).unwrap();
    assert!(clip.audio_cues.is_empty());
    assert!(p
        .to_svg_embed("", splinely_document::EmbedFormat::Json)
        .unwrap()
        .contains("audioCues"));
    assert_eq!(
        Project::from_svg_metadata(&p.to_svg("").unwrap()).unwrap(),
        p
    );
}
#[test]
fn shared_allocators_skip_clip_cues_in_every_clip() {
    let mut p = project();
    p.clips[0].audio_cues[0].id = "node-1".into();
    let mut second = p.clips[0].clone();
    second.id = "second".into();
    second.audio_cues[0].id = "node-2".into();
    p.clips.push(second);
    assert_eq!(p.unused_id("node"), "node-3");
    assert_eq!(p.unused_ids("node", 2), ["node-3", "node-4"]);
    p.clips[0].audio_cues[0].id = "shape-gradient".into();
    p.clips[1].audio_cues[0].id = "shape-gradient_".into();
    assert_eq!(p.gradient_id("shape"), "shape-gradient__");
    p.validate().unwrap();
}

#[test]
fn clip_cues_share_global_id_namespace_and_validate_references_and_ranges() {
    let p = project();
    for cue in [
        AudioCue {
            id: "intro".into(),
            ..p.clips[0].audio_cues[0].clone()
        },
        AudioCue {
            asset_id: "missing".into(),
            ..p.clips[0].audio_cues[0].clone()
        },
        AudioCue {
            time: 2.,
            ..p.clips[0].audio_cues[0].clone()
        },
        AudioCue {
            offset: -1.,
            ..p.clips[0].audio_cues[0].clone()
        },
        AudioCue {
            duration: Some(0.),
            ..p.clips[0].audio_cues[0].clone()
        },
        AudioCue {
            volume: 1.1,
            ..p.clips[0].audio_cues[0].clone()
        },
    ] {
        let mut next = p.clone();
        next.clips[0].audio_cues[0] = cue;
        assert!(next.validate().is_err());
    }
    let mut next = p.clone();
    next.audio_cues.push(p.clips[0].audio_cues[0].clone());
    assert!(next.validate().is_err());
}
