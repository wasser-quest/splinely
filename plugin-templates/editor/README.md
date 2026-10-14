# Trusted static editor plugin

This is **source only**. The CLI never builds, executes, dynamically loads, or
automatically installs it. Review all source and dependencies before manually
building it. Native Rust plugins have the editor process's full privileges:
there is no sandbox, signature check, runtime permission system or isolation.

The generated Cargo manifest points to the local `forma_editor` library and
uses its Bevy ECS version. Move the crate only after adjusting that path.
Explicit, optional verification (runs build scripts and tests; trusted code only):

```sh
cargo test --manifest-path Cargo.toml
```

To install, a trusted native editor host must add this crate as a dependency and
explicitly call its public `register(&mut projection)?` function on its
`forma_editor::ecs::Projection`. Registration uses the public `EditorPlugin`
trait and `Projection::add_plugin`; duplicate plugin types are rejected.
Then set `projection.resource_mut::<Settings>().unwrap().enabled = true`
using this crate's public `Settings` type.

Systems run in `Phase::Decorate` after composition, before render collection.
They query only `DocumentSceneEntity` nodes. Composition resets scene values
each frame, so decoration must be reapplied. Resources survive document reset;
observe `SessionGeneration` if you add document-specific state.

This extension is **editor-only**: preview decorations do not change project
JSON, do not persist, and do not appear in SVG/browser exports. Project JSON or
authored JavaScript cannot request native plugin loading. Browser extensions
require a separately designed runtime implementation, not this native scaffold.
