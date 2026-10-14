//! A disposable scene projection: document IDs, never ECS entity IDs, identify edits.
use bevy_ecs::prelude::*;
use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
};

#[derive(Component, Clone, Debug)]
pub struct SceneNode {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub opacity: f32,
    pub fill: String,
    /// Gradient fill replacing `fill` when present.
    pub gradient: Option<forma_document::Gradient>,
    pub stroke: String,
    pub stroke_width: f32,
    pub shape: Shape,
}

#[derive(Clone, Debug)]
pub enum Shape {
    Rect {
        width: f32,
        height: f32,
        rx: f32,
    },
    Path {
        d: String,
        bounds: [f32; 4],
        evenodd: bool,
    },
    Circle {
        radius: f32,
    },
    Text {
        text: String,
        size: f32,
    },
}

#[derive(Resource, Default)]
struct PendingScene(Vec<SceneNode>);

#[derive(Resource, Default)]
struct PreviewInput(Option<PreviewFrame>);
struct PreviewFrame {
    project: forma_document::Project,
    time: f64,
    context: forma_document::EvaluationContext,
}
#[derive(Resource, Default)]
struct PreviewContext(forma_document::EvaluationContext);

pub struct StateMachinePlugin;
impl EditorPlugin for StateMachinePlugin {
    fn build(&self, world: &mut World, schedule: &mut Schedule) {
        world.init_resource::<PreviewInput>();
        world.init_resource::<PreviewContext>();
        schedule.add_systems(state_values.in_set(Phase::Behavior));
    }
}
fn state_values(input: Res<PreviewInput>, mut context: ResMut<PreviewContext>) {
    if let Some(frame) = &input.0 {
        context.0 = frame.context.clone();
    }
}
pub struct TimelinePlugin;
impl EditorPlugin for TimelinePlugin {
    fn build(&self, _world: &mut World, schedule: &mut Schedule) {
        schedule.add_systems(evaluate.in_set(Phase::Animation));
    }
}
fn evaluate(
    input: Res<PreviewInput>,
    context: Res<PreviewContext>,
    mut pending: ResMut<PendingScene>,
    mut diagnostics: ResMut<PreviewDiagnostics>,
) {
    if let Some(frame) = &input.0 {
        let nodes = frame
            .project
            .evaluate_with_context(frame.time, &context.0)
            .unwrap_or_else(|error| {
                diagnostics
                    .0
                    .push(format!("Vorschau-Ersatzdarstellung: {error}"));
                frame.project.nodes.clone()
            });
        pending.0 = nodes
            .iter()
            .filter_map(crate::projection::project_node)
            .collect();
    }
}
#[derive(Resource, Default)]
struct EntityMap(HashMap<String, Entity>);
#[derive(Resource, Default)]
struct RenderScene(Vec<SceneNode>);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Phase {
    Input,
    Behavior,
    Animation,
    Compose,
    /// Decorators run after document reconciliation and before collection.
    Decorate,
    Render,
}

/// Statically linked plugins install resources and systems in explicit phases.
pub trait EditorPlugin: 'static {
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
    /// All current plugins are editor-only; they must not promise export persistence.
    fn build(&self, world: &mut World, schedule: &mut Schedule);
}
pub struct ScenePlugin;
impl EditorPlugin for ScenePlugin {
    fn build(&self, world: &mut World, schedule: &mut Schedule) {
        world.init_resource::<PendingScene>();
        world.init_resource::<EntityMap>();
        world.init_resource::<RenderScene>();
        schedule.add_systems(project.in_set(Phase::Compose));
        schedule.add_systems(collect.in_set(Phase::Render));
    }
}

fn project(world: &mut World) {
    let nodes = std::mem::take(&mut world.resource_mut::<PendingScene>().0);
    let mut mapping = world.remove_resource::<EntityMap>().unwrap();
    let ids: HashSet<_> = nodes.iter().map(|n| n.id.clone()).collect();
    mapping.0.retain(|id, entity| {
        if ids.contains(id) {
            true
        } else {
            world.despawn(*entity);
            false
        }
    });
    for node in nodes {
        if let Some(entity) = mapping.0.get(&node.id) {
            if let Ok(mut entity_ref) = world.get_entity_mut(*entity) {
                entity_ref.insert(node);
            } else {
                let id = node.id.clone();
                mapping
                    .0
                    .insert(id, world.spawn((node, DocumentSceneEntity)).id());
            }
        } else {
            let id = node.id.clone();
            mapping
                .0
                .insert(id, world.spawn((node, DocumentSceneEntity)).id());
        }
    }
    world.insert_resource(mapping);
}
fn collect(nodes: Query<&SceneNode, With<DocumentSceneEntity>>, mut output: ResMut<RenderScene>) {
    output.0 = nodes.iter().cloned().collect();
}

#[derive(Component)]
pub struct DocumentSceneEntity;
#[derive(Resource, Default)]
pub struct PreviewDiagnostics(pub Vec<String>);
#[derive(Resource, Default)]
pub struct SessionGeneration(pub u64);

pub struct Projection {
    world: World,
    schedule: Schedule,
    plugins: HashSet<TypeId>,
    names: Vec<&'static str>,
}
impl Default for Projection {
    fn default() -> Self {
        let mut world = World::new();
        let mut schedule = Schedule::default();
        schedule.configure_sets(
            (
                Phase::Input,
                Phase::Behavior,
                Phase::Animation,
                Phase::Compose,
                Phase::Decorate,
                Phase::Render,
            )
                .chain(),
        );
        world.init_resource::<PreviewDiagnostics>();
        world.init_resource::<SessionGeneration>();
        let mut projection = Self {
            world,
            schedule,
            plugins: HashSet::new(),
            names: vec![],
        };
        projection.add_plugin(StateMachinePlugin).unwrap();
        projection.add_plugin(TimelinePlugin).unwrap();
        projection.add_plugin(ScenePlugin).unwrap();
        projection
    }
}
impl Projection {
    /// Plugins are trusted native code. Duplicate types are rejected before build runs.
    pub fn add_plugin<P: EditorPlugin>(&mut self, plugin: P) -> Result<(), String> {
        if !self.plugins.insert(TypeId::of::<P>()) {
            return Err(format!("Plugin already installed: {}", plugin.name()));
        }
        self.names.push(plugin.name());
        plugin.build(&mut self.world, &mut self.schedule);
        Ok(())
    }
    pub fn plugin_names(&self) -> &[&'static str] {
        &self.names
    }
    pub fn resource_mut<R: Resource>(&mut self) -> Option<Mut<'_, R>> {
        self.world.get_resource_mut::<R>()
    }
    pub fn diagnostics(&self) -> &[String] {
        &self.world.resource::<PreviewDiagnostics>().0
    }
    /// Clears document entities, not unrelated plugin entities. Resource plugins can observe generation.
    pub fn reset_document(&mut self) {
        let entities: Vec<_> = self
            .world
            .resource::<EntityMap>()
            .0
            .values()
            .copied()
            .collect();
        for entity in entities {
            self.world.despawn(entity);
        }
        self.world.resource_mut::<EntityMap>().0.clear();
        self.world.resource_mut::<RenderScene>().0.clear();
        self.world.resource_mut::<PreviewInput>().0 = None;
        self.world.resource_mut::<SessionGeneration>().0 += 1;
    }
    pub fn evaluate_project(
        &mut self,
        project: &forma_document::Project,
        time: f64,
        clip: Option<String>,
        animate: bool,
        states: std::collections::BTreeMap<String, String>,
        runtime_values: Vec<forma_document::PropertyOverride>,
    ) -> Vec<SceneNode> {
        let context = forma_document::EvaluationContext {
            states,
            overrides: runtime_values,
            clips: Some(if animate {
                clip.into_iter()
                    .map(|clip_id| forma_document::ClipPlayback { clip_id, time })
                    .collect()
            } else {
                vec![]
            }),
            ..Default::default()
        };
        self.evaluate_with_context(project, time, &context)
    }
    /// Full native playback context; legacy evaluate_project remains source compatible.
    pub fn evaluate_with_context(
        &mut self,
        project: &forma_document::Project,
        time: f64,
        context: &forma_document::EvaluationContext,
    ) -> Vec<SceneNode> {
        self.world.resource_mut::<PreviewDiagnostics>().0.clear();
        self.world.resource_mut::<PreviewInput>().0 = Some(PreviewFrame {
            project: project.clone(),
            time,
            context: context.clone(),
        });
        self.schedule.run(&mut self.world);
        let order: HashMap<_, _> = project
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        let mut result = self.world.resource::<RenderScene>().0.clone();
        result.sort_by_key(|n| order.get(&n.id).copied().unwrap_or(usize::MAX));
        result
    }
    #[cfg(test)]
    fn update(&mut self, nodes: Vec<SceneNode>) -> Vec<SceneNode> {
        self.world.resource_mut::<PreviewInput>().0 = None;
        let order: HashMap<_, _> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        self.world.resource_mut::<PendingScene>().0 = nodes;
        self.schedule.run(&mut self.world);
        let mut result = self.world.resource::<RenderScene>().0.clone();
        result.sort_by_key(|node| order.get(&node.id).copied().unwrap_or(usize::MAX));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: &str) -> SceneNode {
        SceneNode {
            id: id.into(),
            x: 0.,
            y: 0.,
            rotation: 0.,
            scale_x: 1.,
            scale_y: 1.,
            opacity: 1.,
            fill: "#ffffff".into(),
            gradient: None,
            stroke: "none".into(),
            stroke_width: 0.,
            shape: Shape::Circle { radius: 20. },
        }
    }
    #[test]
    fn plugin_decorates_after_compose_and_is_editor_only() {
        use crate::plugins::*;
        let mut p = Projection::default();
        p.add_plugin(SelectionHighlightPlugin).unwrap();
        assert!(p.add_plugin(SelectionHighlightPlugin).is_err());
        *p.resource_mut::<SelectionHighlight>().unwrap() = SelectionHighlight {
            enabled: true,
            selected: Some("a".into()),
        };
        assert_eq!(p.update(vec![node("a")])[0].stroke, "#38bdf8ff");
        p.resource_mut::<SelectionHighlight>().unwrap().enabled = false;
        assert_eq!(p.update(vec![node("a")])[0].stroke, "none");
        let old = p.world.resource::<EntityMap>().0["a"];
        let unrelated = p.world.spawn_empty().id();
        p.reset_document();
        assert!(p.world.get_entity(unrelated).is_ok());
        assert!(p.world.get_entity(old).is_err());
        assert_eq!(p.world.resource::<SessionGeneration>().0, 1);
        p.update(vec![node("a")]);
        assert_ne!(p.world.resource::<EntityMap>().0["a"], old);
    }
    #[test]
    fn projection_preserves_entities_and_removes_deleted_nodes() {
        let mut p = Projection::default();
        p.update(vec![node("a"), node("b")]);
        let a = p.world.resource::<EntityMap>().0["a"];
        p.update(vec![node("a")]);
        assert_eq!(p.world.resource::<EntityMap>().0["a"], a);
        assert_eq!(p.world.resource::<EntityMap>().0.len(), 1);
    }
}
