//! Small, atomic authoring candidates. Nothing here mutates the input project.
use forma_document::*;

fn validated(project: Project) -> Result<Project, String> {
    project.validate().map_err(|e| e.to_string())?;
    Ok(project)
}

fn editable(project: &Project, target: &str) -> Result<(), String> {
    if let Some(node) = project.nodes.iter().find(|n| n.id == target) {
        if node.locked {
            return Err("Diese Form ist geschützt. Zuerst entsperren.".into());
        }
        return Ok(());
    }
    if project.bones.iter().any(|b| b.id == target) {
        return Ok(());
    }
    Err("Bitte eine Form oder einen Knochen auswählen.".into())
}

pub(super) fn keyframe_candidate(
    project: &Project,
    clip_id: &str,
    target: &str,
    property: Property,
    time: f64,
    value: AnimValue,
) -> Result<Project, String> {
    editable(project, target)?;
    let mut next = project.clone();
    let clip = next
        .clips
        .iter_mut()
        .find(|c| c.id == clip_id)
        .ok_or("Bitte zuerst einen Clip erstellen.")?;
    if !time.is_finite() || time < 0. || time > clip.duration {
        return Err("Zeit muss innerhalb des Clips liegen.".into());
    }
    if !clip
        .tracks
        .iter()
        .any(|t| t.node_id == target && t.property == property)
    {
        clip.tracks.push(Track {
            node_id: target.into(),
            property,
            keyframes: vec![],
        });
    }
    let track = clip
        .tracks
        .iter_mut()
        .find(|t| t.node_id == target && t.property == property)
        .unwrap();
    if let Some(old) = track.keyframes.iter_mut().find(|k| k.time == time) {
        old.value = value;
    } else {
        track.keyframes.push(Keyframe {
            time,
            value,
            easing: Easing::Linear,
        });
    }
    track.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
    validated(next)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Preset {
    Bounce,
    Wave,
    Opacity,
}

/// Creates one new track across the selected clip, never overwriting authored keys.
pub(super) fn preset_candidate(
    project: &Project,
    clip_id: &str,
    target: &str,
    preset: Preset,
    base: f64,
) -> Result<Project, String> {
    editable(project, target)?;
    let property = match preset {
        Preset::Bounce => Property::Y,
        Preset::Wave => Property::Rotation,
        Preset::Opacity => Property::Opacity,
    };
    let clip = project
        .clips
        .iter()
        .find(|c| c.id == clip_id)
        .ok_or("Bitte zuerst einen Clip erstellen.")?;
    if clip
        .tracks
        .iter()
        .any(|t| t.node_id == target && t.property == property)
    {
        return Err("Diese Spur enthält schon Schlüssel. Einen neuen Clip wählen oder die Spur bewusst im Expertenbereich entfernen.".into());
    }
    let middle = match preset {
        Preset::Bounce => base - 30.,
        Preset::Wave => base + 20.,
        Preset::Opacity => 0.,
    };
    let mut next = project.clone();
    for (time, value) in [
        (0., base),
        (clip.duration * 0.5, middle),
        (clip.duration, base),
    ] {
        next = keyframe_candidate(
            &next,
            clip_id,
            target,
            property,
            time,
            AnimValue::Number(value),
        )?;
    }
    Ok(next)
}

#[derive(Clone, Debug, Default)]
pub(super) struct EntryClip {
    pub clip_id: String,
    pub fade: bool,
    pub weight: f64,
    pub duration: f64,
}

fn entry_actions(entry: &EntryClip) -> Vec<Action> {
    if entry.clip_id.is_empty() {
        return vec![];
    }
    let mut actions = vec![Action::PlayClip {
        clip_id: entry.clip_id.clone(),
    }];
    if entry.fade {
        actions.push(Action::BlendClip {
            clip_id: entry.clip_id.clone(),
            weight: entry.weight,
            duration: entry.duration,
        });
    }
    actions
}

#[derive(Clone)]
pub(super) struct MachineDraft {
    pub idle: String,
    pub active: String,
    pub initial_active: bool,
    pub target: String,
    pub hover: bool,
    pub idle_clip: EntryClip,
    pub active_clip: EntryClip,
}
impl Default for MachineDraft {
    fn default() -> Self {
        Self {
            idle: "Ruhe".into(),
            active: "Aktiv".into(),
            initial_active: false,
            target: String::new(),
            hover: false,
            idle_clip: EntryClip {
                weight: 1.,
                duration: 0.3,
                ..Default::default()
            },
            active_clip: EntryClip {
                weight: 1.,
                duration: 0.3,
                ..Default::default()
            },
        }
    }
}

pub(super) fn machine_candidate(
    project: &Project,
    id: &str,
    draft: &MachineDraft,
) -> Result<Project, String> {
    if draft.idle.trim().is_empty() || draft.active.trim().is_empty() {
        return Err("Beide Zustände brauchen einen Namen.".into());
    }
    if !project.nodes.iter().any(|n| n.id == draft.target) {
        return Err("Als Ereignisziel eine Form wählen (kein Knochen).".into());
    }
    let mut next = project.clone();
    next.state_machines.push(StateMachine {
        id: id.into(),
        initial: if draft.initial_active {
            draft.active.clone()
        } else {
            draft.idle.clone()
        },
        states: vec![
            State {
                id: draft.idle.clone(),
                on_enter: entry_actions(&draft.idle_clip),
                overrides: vec![],
            },
            State {
                id: draft.active.clone(),
                on_enter: entry_actions(&draft.active_clip),
                overrides: vec![],
            },
        ],
        transitions: vec![Transition {
            from: draft.idle.clone(),
            to: draft.active.clone(),
            event: Event {
                name: if draft.hover { "pointerenter" } else { "click" }.into(),
                target: Some(draft.target.clone()),
            },
            // No guard means unconditional event matching, not an invented variable condition.
            guard: None,
            actions: vec![],
        }],
    });
    validated(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project() -> Project {
        let mut p = Project::default();
        p.nodes.push(Node::circle("ball", 0., 0., 10.));
        p.clips.push(Clip {
            id: "move".into(),
            name: "Move".into(),
            duration: 2.,
            looping: false,
            autoplay: false,
            blend_mode: BlendMode::Override,
            weight: 1.,
            tracks: vec![],
        });
        p
    }
    #[test]
    fn keys_replace_sort_and_reject_invalid_without_mutation() {
        let original = project();
        let p = keyframe_candidate(
            &original,
            "move",
            "ball",
            Property::X,
            1.,
            AnimValue::Number(4.),
        )
        .unwrap();
        let p =
            keyframe_candidate(&p, "move", "ball", Property::X, 0., AnimValue::Number(2.)).unwrap();
        let p =
            keyframe_candidate(&p, "move", "ball", Property::X, 1., AnimValue::Number(8.)).unwrap();
        let keys = &p.clips[0].tracks[0].keyframes;
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[1].value, AnimValue::Number(8.));
        for time in [f64::NAN, -1., 3.] {
            assert!(keyframe_candidate(
                &p,
                "move",
                "ball",
                Property::X,
                time,
                AnimValue::Number(0.)
            )
            .is_err());
        }
        assert!(keyframe_candidate(
            &p,
            "move",
            "ball",
            Property::Opacity,
            0.,
            AnimValue::Number(2.)
        )
        .is_err());
        assert!(original.clips[0].tracks.is_empty());
    }
    #[test]
    fn presets_are_atomic_and_never_replace_tracks_or_locked_nodes() {
        let mut p = project();
        for preset in [Preset::Bounce, Preset::Wave, Preset::Opacity] {
            let next = preset_candidate(&p, "move", "ball", preset, 1.).unwrap();
            assert_eq!(next.clips[0].tracks[0].keyframes.len(), 3);
            assert!(preset_candidate(&next, "move", "ball", preset, 1.).is_err());
        }
        assert!(preset_candidate(&p, "move", "ball", Preset::Opacity, 2.).is_err());
        p.nodes[0].locked = true;
        assert!(preset_candidate(&p, "move", "ball", Preset::Bounce, 0.).is_err());
        assert!(
            keyframe_candidate(&p, "move", "ball", Property::X, 0., AnimValue::Number(0.)).is_err()
        );
        assert!(p.clips[0].tracks.is_empty());
    }
    #[test]
    fn bones_use_only_local_numeric_channels_and_preserve_source() {
        let mut p = project();
        p.raw_svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into();
        p.custom_js = "// unchanged".into();
        p.bones.push(Bone {
            id: "arm".into(),
            parent_id: None,
            x: 3.,
            y: 4.,
            rotation: 5.,
            length: 10.,
        });
        let next = preset_candidate(&p, "move", "arm", Preset::Wave, 5.).unwrap();
        assert_eq!(next.clips[0].tracks[0].node_id, "arm");
        assert_eq!(next.raw_svg, p.raw_svg);
        assert_eq!(next.custom_js, p.custom_js);
        assert_eq!(next.bones, p.bones);
        assert!(preset_candidate(&p, "move", "arm", Preset::Opacity, 1.).is_err());
        assert!(keyframe_candidate(
            &p,
            "move",
            "arm",
            Property::Fill,
            0.,
            AnimValue::Color(Color::rgb(0, 0, 0))
        )
        .is_err());
        let draft = MachineDraft {
            target: "arm".into(),
            ..Default::default()
        };
        assert!(machine_candidate(&p, "machine", &draft).is_err());
    }
    #[test]
    fn small_duration_presets_keep_distinct_keys_and_existing_easing() {
        let mut p = project();
        p.clips[0].duration = 1e-7;
        let mut next = preset_candidate(&p, "move", "ball", Preset::Bounce, 0.).unwrap();
        assert_eq!(next.clips[0].tracks[0].keyframes.len(), 3);
        next.clips[0].tracks[0].keyframes[0].easing = Easing::Step;
        let next = keyframe_candidate(
            &next,
            "move",
            "ball",
            Property::Y,
            0.,
            AnimValue::Number(4.),
        )
        .unwrap();
        assert_eq!(next.clips[0].tracks[0].keyframes[0].easing, Easing::Step);
    }
    #[test]
    fn machine_uses_real_target_guard_and_weighted_entry_actions() {
        let mut p = project();
        let mut draft = MachineDraft {
            target: "ball".into(),
            hover: true,
            ..Default::default()
        };
        draft.active_clip.clip_id = "move".into();
        draft.active_clip.fade = true;
        assert!(machine_candidate(&p, "machine", &draft).is_err());
        p.clips[0].blend_mode = BlendMode::Weighted;
        let next = machine_candidate(&p, "machine", &draft).unwrap();
        let m = &next.state_machines[0];
        assert_eq!(m.initial, draft.idle);
        assert_eq!(m.transitions[0].event.name, "pointerenter");
        assert_eq!(m.transitions[0].event.target.as_deref(), Some("ball"));
        assert!(m.transitions[0].guard.is_none());
        assert!(matches!(
            m.states[1].on_enter.as_slice(),
            [Action::PlayClip { .. }, Action::BlendClip { .. }]
        ));
        draft.initial_active = true;
        assert_eq!(
            machine_candidate(&p, "machine", &draft)
                .unwrap()
                .state_machines[0]
                .initial,
            draft.active
        );
        draft.active = draft.idle.clone();
        assert!(machine_candidate(&p, "machine", &draft).is_err());
        assert!(p.state_machines.is_empty());
    }
}
