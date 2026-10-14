//! Example trusted editor-only extension: selected-node stroke decoration.
use crate::ecs::{DocumentSceneEntity, EditorPlugin, Phase, SceneNode};
use bevy_ecs::prelude::*;
#[derive(Resource, Default)]
pub struct SelectionHighlight {
    pub enabled: bool,
    pub selected: Option<String>,
}
pub struct SelectionHighlightPlugin;
impl EditorPlugin for SelectionHighlightPlugin {
    fn build(&self, world: &mut World, schedule: &mut Schedule) {
        world.init_resource::<SelectionHighlight>();
        schedule.add_systems(highlight.in_set(Phase::Decorate));
    }
}
fn highlight(
    settings: Res<SelectionHighlight>,
    mut nodes: Query<&mut SceneNode, With<DocumentSceneEntity>>,
) {
    if !settings.enabled {
        return;
    }
    for mut node in &mut nodes {
        if settings.selected.as_deref() == Some(&node.id) {
            node.stroke = "#38bdf8ff".into();
            node.stroke_width = 3.;
        }
    }
}
