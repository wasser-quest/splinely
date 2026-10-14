//! Trusted, statically linked, editor-only preview decoration.
//! This code is not a sandbox and does not persist changes into JSON or SVG.
use bevy_ecs::prelude::*;
use forma_editor::ecs::{DocumentSceneEntity, EditorPlugin, Phase, Projection, SceneNode};

#[derive(Resource, Default)]
pub struct Settings {
    pub enabled: bool,
}

pub struct GeneratedPlugin;

impl EditorPlugin for GeneratedPlugin {
    fn build(&self, world: &mut World, schedule: &mut Schedule) {
        world.init_resource::<Settings>();
        schedule.add_systems(decorate.in_set(Phase::Decorate));
    }
}

/// Call explicitly from a trusted native editor host, never from project JSON.
pub fn register(projection: &mut Projection) -> Result<(), String> {
    projection.add_plugin(GeneratedPlugin)
}

fn decorate(settings: Res<Settings>, mut nodes: Query<&mut SceneNode, With<DocumentSceneEntity>>) {
    if !settings.enabled {
        return;
    }
    for mut node in &mut nodes {
        node.stroke = "#38bdf8ff".into();
        node.stroke_width = 3.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_registration_rejects_duplicates() {
        let mut projection = Projection::default();
        register(&mut projection).unwrap();
        assert!(register(&mut projection).is_err());
        projection.resource_mut::<Settings>().unwrap().enabled = true;
    }
}
