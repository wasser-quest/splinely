use splinely_document::{AudioAsset, AudioCue, Project};
use splinely_runtime::{HostAction, Request, Response, Runtime};

fn project(looping: bool) -> Project {
    let mut p = Project::default();
    p.audio_assets.push(AudioAsset {
        id: "sound".into(),
        name: String::new(),
        mime_type: "audio/wav".into(),
        data_base64: "AQID".into(),
    });
    p.clips.push(
        serde_json::from_value(serde_json::json!({
            "id":"clip", "duration":2, "looping":looping, "autoplay":true,
            "blendMode":"weighted", "weight":0,
            "audioCues":[
                {"id":"zero", "assetId":"sound", "time":0, "offset":0, "volume":1, "duration":0.25},
                {"id":"hit", "assetId":"sound", "time":0.5, "offset":1, "volume":1, "duration":9}
            ]
        }))
        .unwrap(),
    );
    p
}
fn cues(r: Response) -> Vec<AudioCue> {
    r.host_actions
        .into_iter()
        .filter_map(|a| match a {
            HostAction::ClipAudioCue { clip_id, cue } => {
                assert_eq!(clip_id, "clip");
                Some(cue)
            }
            _ => None,
        })
        .collect()
}
#[test]
fn boundaries_zero_repeated_loops_and_truncation_ignore_visual_weight() {
    let mut r = Runtime::new(project(true)).unwrap();
    assert_eq!(cues(r.tick(0.0).unwrap())[0].id, "zero");
    assert!(cues(r.tick(0.0).unwrap()).is_empty());
    assert!(cues(r.tick(0.49).unwrap()).is_empty());
    let hit = cues(r.tick(0.01).unwrap());
    assert_eq!(hit[0].id, "hit");
    assert_eq!(hit[0].duration, Some(1.5));
    let rest = cues(r.tick(4.0).unwrap());
    assert_eq!(
        rest.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ["zero", "hit", "zero", "hit"]
    );
}
#[test]
fn playing_seek_resumes_overlap_paused_seek_silent_end_exclusive_and_replay() {
    let mut r = Runtime::new(project(false)).unwrap();
    let hit = cues(
        r.execute(Request::SeekClip {
            clip_id: "clip".into(),
            time: 1.0,
        })
        .unwrap(),
    );
    assert_eq!((hit[0].offset, hit[0].duration), (1.5, Some(1.0)));
    assert!(cues(r.tick(0.0).unwrap()).is_empty());
    r.execute(Request::PauseClip {
        clip_id: "clip".into(),
    })
    .unwrap();
    assert!(cues(
        r.execute(Request::SeekClip {
            clip_id: "clip".into(),
            time: 0.75
        })
        .unwrap()
    )
    .is_empty());
    r.execute(Request::PlayClip {
        clip_id: "clip".into(),
    })
    .unwrap();
    assert_eq!(cues(r.tick(0.0).unwrap())[0].offset, 1.25);
    assert!(cues(r.tick(1.25).unwrap()).is_empty());
    assert!(!r.evaluate().unwrap().clips[0].playing);
    r.execute(Request::PlayClip {
        clip_id: "clip".into(),
    })
    .unwrap();
    assert_eq!(cues(r.tick(0.0).unwrap())[0].id, "zero");
    assert!(cues(
        r.execute(Request::SeekClip {
            clip_id: "clip".into(),
            time: 2.0
        })
        .unwrap()
    )
    .is_empty());
}
#[test]
fn global_seek_and_pause_keep_global_scheduler_silent_and_reconstruct_clip_overlap() {
    let mut p = project(true);
    let mut cue = p.clips[0].audio_cues[0].clone();
    cue.id = "global".into();
    p.audio_cues.push(cue);
    let mut r = Runtime::new(p).unwrap();
    r.execute(Request::Pause).unwrap();
    assert!(r
        .execute(Request::Seek { time: 1.0 })
        .unwrap()
        .host_actions
        .is_empty());
    assert!(r.tick(1.0).unwrap().host_actions.is_empty());
    r.execute(Request::Play).unwrap();
    let out = r.tick(0.0).unwrap();
    assert!(!out
        .host_actions
        .iter()
        .any(|a| matches!(a, HostAction::AudioCue { .. })));
    assert_eq!(cues(out)[0].offset, 1.5);
    let seek = cues(r.execute(Request::Seek { time: 2.0 }).unwrap());
    assert_eq!(seek[0].id, "zero");
}
#[test]
fn oversized_catch_up_rolls_back_clip_cue_state() {
    let mut r = Runtime::new(project(true)).unwrap();
    assert!(r.tick(5000.).is_err());
    assert_eq!(r.evaluate().unwrap().time, 0.);
    assert_eq!(cues(r.tick(0.).unwrap())[0].id, "zero");
}

#[test]
fn independent_clips_stop_and_wire_scope() {
    let mut p = project(true);
    let mut second = p.clips[0].clone();
    second.id = "other".into();
    second.audio_cues.truncate(1);
    second.audio_cues[0].id = "other-zero".into();
    p.clips.push(second);
    let mut r = Runtime::new(p).unwrap();
    let seek = r
        .execute(Request::SeekClip {
            clip_id: "clip".into(),
            time: 1.,
        })
        .unwrap();
    assert_eq!(
        seek.host_actions.len(),
        1,
        "clip seek must not start another clip's pending cue"
    );
    r.execute(Request::StopClip {
        clip_id: "clip".into(),
    })
    .unwrap();
    let out = r.tick(0.0).unwrap();
    assert_eq!(out.host_actions.len(), 1);
    let json = serde_json::to_value(&out.host_actions[0]).unwrap();
    assert_eq!(json["type"], "clipAudioCue");
    assert_eq!(json["clipId"], "other");
}
