//! Stable JSON discovery with command metadata sourced from clap, not a second parser.
use crate::{demo, project_value, validated, Result};
use serde_json::{json, Value};

fn command_info(command: &clap::Command) -> Value {
    json!({
        "name":command.get_name(),
        "description":command.get_about().map(ToString::to_string),
        "arguments":command.get_arguments().map(|arg| json!({
            "id":arg.get_id().as_str(),
            "long":arg.get_long(),
            "short":arg.get_short(),
            "positional":arg.is_positional(),
            "required":arg.is_required_set(),
            "description":arg.get_help().map(ToString::to_string),
            "defaults":arg.get_default_values().iter().map(|v| v.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            "values":arg.get_value_parser().possible_values().map(|values| values.map(|v| v.get_name().to_string()).collect::<Vec<_>>())
        })).collect::<Vec<_>>(),
        "subcommands":command.get_subcommands().map(command_info).collect::<Vec<_>>()
    })
}

pub fn model() -> Value {
    json!({
        "notation":"Field lists describe canonical JSON, not a formal JSON Schema. Use schema examples and validate as the authority. Fields with defaults may be omitted; Transform and Style also default each omitted member. Other nested fields are required.",
        "project":{"fields":["schemaVersion","name","width","height","nodes","bones","clips","blendSpaces","stateMachines","variables","audioAssets","audioCues","customJs","rawSvg","guides","followers","svgImport","description"],"defaults":{"schemaVersion":1,"name":"Untitled","width":800,"height":600,"nodes":[],"bones":[],"clips":[],"blendSpaces":[],"stateMachines":[],"variables":{},"audioAssets":[],"audioCues":[],"customJs":"","rawSvg":""}},
        "node":{"required":["id","kind"],"defaults":{"name":"","boneId":null,"visible":true,"locked":false,"originalKind":null,"group":null,"gradient":null,"transform":{"x":0,"y":0,"rotation":0,"scaleX":1,"scaleY":1},"style":{"fill":{"r":0.3137254901960784,"g":0.5490196078431373,"b":0.9411764705882353,"a":1},"stroke":null,"strokeWidth":1,"opacity":1}}},
        "transform":{"defaults":{"x":0,"y":0,"rotation":0,"scaleX":1,"scaleY":1},"units":"SVG units; rotation degrees; node translation separate from geometry"},
        "style":{"defaults":{"fill":{"r":0.3137254901960784,"g":0.5490196078431373,"b":0.9411764705882353,"a":1},"stroke":null,"strokeWidth":1,"opacity":1}},
        "color":{"required":["r","g","b","a"],"range":[0,1]},
        "group":{"default":null,"constraint":"ID of the node's direct group in project.groups (omitted when null). Exported as nested <g> elements; no effect on rendering."},
        "groups":{"default":[],"fields":{"id":"shared ID namespace","name":"display name, default empty, max 200","parentId":"enclosing group ID or omitted for top level"},"constraint":"Parents must exist, no cycles, depth <= 32. Use `arrange group`/`ungroup` to edit safely; empty groups are removed by those commands."},
        "guides":{"default":[],"fields":{"vertical":"bool: true = vertical line at x = position","position":"finite scene coordinate"},"constraint":"Editor ruler guides, max 1000; not exported as geometry."},
        "gradient":{"default":null,"fields":{"radial":"bool, default false","angle":"degrees, default 0 (0 = left to right, 90 = top to bottom; linear only)","stops":"2..16 of {offset 0..1 nondecreasing, color}"},"constraint":"Replaces style.fill in exports and previews (objectBoundingBox units); fill colour tracks then have no visible effect. Omitted when null."},
        "originalKind":{"default":null,"constraint":"Only on path nodes; convertible rect/circle geometry with positive dimensions/radius. convert-back additionally requires unchanged generated path geometry."},
        "geometry":[{"type":"rect","x":0,"y":0,"width":80,"height":40,"rx":0},{"type":"circle","cx":0,"cy":0,"radius":20},{"type":"text","x":0,"y":0,"text":"Hello","fontSize":16},{"type":"path","d":"M 0 0 L 80 0 Q 100 40 80 80 C 60 100 20 100 0 80 Z","bounds":[0,0,90,95],"evenodd":false}],
        "geometryDefaults":{"rect.rx":0,"path.evenodd":false},
        "bone":{"required":["id","x","y","rotation","length"],"defaults":{"parentId":null}},
        "clip":{"required":["id","duration"],"defaults":{"name":"","looping":false,"autoplay":false,"tracks":[],"blendMode":"override","weight":1}},
        "blendSpace":{"required":["id","parameters","samples"],"parameters":"Exactly two distinct names of declared numeric variables in +/-10000000","samples":"1..256 {clipId,position:[x,y]}; existing weighted clips, owned by at most one sample globally; positions in +/-10000000 and more than 0.000001 apart","weights":"At an exact sample: one-hot. Else normalized inverse squared Euclidean distance; defined outside hull. Multiplies clip weight; never activates clips or changes clocks. Omitted empty project.blendSpaces defaults to [].","command":"blend-space list/set/remove/parameter/weights"},
        "track":{"required":["nodeId","property","keyframes"]},
        "keyframe":{"required":["time","value"],"defaults":{"easing":{"type":"linear"}}},
        "values":[{"type":"number","value":42},{"type":"color","value":{"r":1,"g":0,"b":0,"a":1}},{"type":"gradient","value":{"radial":false,"angle":90,"stops":[{"offset":0,"color":{"r":1,"g":0,"b":0,"a":1}},{"offset":1,"color":{"r":0,"g":0,"b":1,"a":1}}]}},{"type":"path","value":"M0 0 C10 0 20 10 20 20 Z"}],
        "easings":[{"type":"linear"},{"type":"step"},{"type":"cubicBezier","x1":0.42,"y1":0,"x2":0.58,"y2":1}],
        "properties":{"numeric":["motionProgress","x","y","rotation","scaleX","scaleY","opacity","width","height","radius","fontSize"],"color":["fill","stroke"],"gradient":["gradient"],"path":["d"],"structured":"gradient keyframes keep the node gradient's kind and stop count; d keyframes of one track share the same command structure (use `arrange morph`); both interpolate number by number and blend in weighted clips when compatible, otherwise the heaviest contribution wins","bone":["x","y","rotation"],"kindSpecific":{"width":"rect","height":"rect","radius":"circle","fontSize":"text","d":"path","gradient":"nodes with a gradient fill"}},
        "stateMachine":{"required":["id","initial","states"],"defaults":{"transitions":[]}},
        "a11y":{"scope":"Web player and exported SVG only (no effect in the native runtime/editor canvas); opt-in: nothing is added unless project.description, a node a11y or a state announce exists","project.description":"optional string, 1..1000 chars, no control characters or surrounding spaces, omitted when empty; the title is project.name","node.a11y":{"label":"1..200 chars, aria-label (default for interactive nodes: node name or ID)","description":"1..1000 chars","role":"button|link|img|group|heading (interactive nodes default to button)","hidden":"bool: decorative, aria-hidden; cannot be combined with label/description/role or used on interactive nodes"},"state.announce":"1..200 chars; announced politely (aria-live) when the state is entered","interactiveNodes":"nodes targeted by a transition event (click, dblclick, pointer*, key*, drag*, ...) become focusable (tabindex 0); Enter/Space dispatch click","commands":"splinely-cli a11y FILE ...; check warns interactive_without_label"},
        "state":{"required":["id"],"defaults":{"onEnter":[],"overrides":[],"onExit":[]},"onExit":"Actions run when a transition leaves the state, before the transition's own actions"},
        "transition":{"required":["from","to","event"],"defaults":{"guard":null,"actions":[],"blend":0},"after":"Timer transition: fire this many seconds (0..3600) after entering from; event is then {name:\"after\"} and may be omitted","blend":"Seconds (0..60) over which the old state's overrides crossfade into the new state's; machines blend independently, so several can fade at once"},
        "motionPath":{"nodeField":{"path":"path-node-id","progress":0,"orient":false},"property":"motionProgress (0..1); requires binding","command":"motion-path INPUT --target NODE --path PATH [--progress 0.5] [--orient] --output FILE; --remove removes binding (remove referencing tracks/overrides first)","semantics":"Adds sampled scene point to target translation, tangent degrees to rotation when orient=true. Target local origin follows the path. Sources and targets cannot be bone-bound; sources cannot themselves have motionPath. M/L/Q/C/Z only; 64 chords per segment, disconnected subpaths concatenate without gaps."},
        "follower":{"required":["id","target","max"],"defaults":{"mode":"offset","strength":1,"smoothing":0,"restAngle":0,"origin":null,"when":null},"modes":{"offset":"adds to x/y towards the pointer, length clamped to max (SVG units)","rotate":"adds to rotation towards the pointer relative to restAngle, clamped to ±max degrees (≤180)"},"when":{"machine":"machine id","states":["only follow in these states"]},"note":"Applied after clips, states and explicit changes; smoothing (0..10 s) also fades it in/out when its states start or end. Targets a node or bone. Command: follow"},
        "event":{"required":["name"],"defaults":{"target":null}},
        "guard":{"required":["variable","equals"]},
        "propertyOverride":{"required":["nodeId","property","value"]},
        "actions":[{"type":"setProperty","nodeId":"ball","property":"opacity","value":{"type":"number","value":0.5}},{"type":"playClip","clipId":"travel"},{"type":"stopClip","clipId":"travel"},{"type":"blendClip","clipId":"overlay","weight":0.5,"duration":0.25},{"type":"setVariable","name":"enabled","value":true},{"type":"emit","event":{"name":"GO","target":null}},{"type":"playAudio","assetId":"voice"}],
        "actionDefaults":{"blendClip.duration":0},
        "audioAsset":{"required":["id","name","mimeType","dataBase64"]},
        "audioCue":{"required":["id","assetId","time","offset","volume"],"optional":{"duration":null}},
        "constraints":{"ids":"^[A-Za-z_][A-Za-z0-9_.-]*$; nodes/bones/clips/machines/assets/cues share one namespace; state IDs are machine-local; node splinely-project reserved","numbers":"Finite; geometry/transforms and numeric animation values within +/-10000000; sizes and strokeWidth 0..10000000; colors/opacity/weights 0..1","canvas":"Width/height >0 and <=1e30","bones":"Finite base coordinates/rotation within +/-1e30, positive length <=1e30; acyclic depth <=128","clips":"Finite duration >0 (no fixed upper bound); key times strictly increasing in [0,duration]; nonempty tracks unique by target/property; cubic easing controls 0..1","variables":"Nonempty names and scalar JSON only; guards/setVariable reference declared variable with same type","events":"Nonempty name; optional target must be a node ID","audio":"Canonical base64 and audio/subtype MIME; time/offset 0..10000000; volume 0..1; optional duration finite >0 (no fixed upper bound)","paths":"Tight local [x,y,width,height] bounds excluding stroke, calculated by shared path_bounds; each component tolerance 1e-4*max(abs(authored),abs(computed),1); geometry must be drawable. CLI path edits recompute bounds; unsupported edit commands H/V/S/T/A fail"},
        "evaluationContext":{"defaults":{"states":{},"clips":null,"clipWeights":{},"variables":{},"overrides":[]},"example":{"states":{"interaction":"clicked"},"clips":[{"clipId":"travel","time":0.5},{"clipId":"overlay","time":1.25}],"clipWeights":{"overlay":0.25},"overrides":[{"nodeId":"ball","property":"opacity","value":{"type":"number","value":0.9}}]},"semantics":"Missing/null clips selects autoplay at --time; [] disables clips; ordered entries have independent finite nonnegative clocks (loops wrap, nonloops clamp). Missing states use initial. All IDs/weights including inactive entries validate. Override-mode weights are ignored. variables supplies blend-space parameter overrides (missing uses authored variables). Explicit overrides apply last; no actions/transitions/fade clocks execute. Duplicate playback entries are accepted by core and sample repeatedly."},
        "blending":{"modes":["override","weighted","additive"],"additive":"B + weight*(sample - immutable authored document rest value), in ordered clip position; flushes weighted groups; numbers and RGBA only, scalar rotation and scale deltas (not multiplicative). RGBA/opacity/motionProgress clamp 0..1; sizes clamp at zero; absent stroke reference transparent black","weightRange":[0,1],"documentDefaults":{"blendMode":"override","weight":1},"commandDefaults":{"mode":"weighted","weight":1},"action":"blendClip requires an existing weighted or additive clip, finite weight 0..1 and duration >=0; does not activate/restart/stop; fade-zero is not stop","ordering":"State overrides, then clips in order, then explicit overrides. Contiguous weighted clips form a group; override and additive clips flush it; bones blend locally before FK","formula":"Per property: S=sum of contributing weights; D=max(1,S); result=B*max(0,1-S)+sum(sample_i*weight_i/D). Missing tracks do not contribute; zero weights preserve baseline; positive stroke uses transparent absent baseline","runtime":"clip.weight, setWeight(weight), fadeTo(weight,seconds); ramps linear, interrupt from current value, continue on paused/ended clips; stop cancels ramp retaining weight; seek preserves ramp; audio volume independent"}
    })
}

pub fn complete_example() -> Result<Value> {
    let mut value = project_value(&demo()?)?;
    value["nodes"].as_array_mut().unwrap().push(json!({"id":"curve","kind":{"type":"path","d":"M 0 0 L 80 0 Q 100 40 80 80 C 60 100 20 100 0 80 Z","bounds":[0,0,90,95],"evenodd":false},"transform":{"x":200,"y":180}}));
    value["bones"] = json!([{"id":"root","x":300,"y":160,"rotation":0,"length":50},{"id":"child","parentId":"root","x":0,"y":0,"rotation":20,"length":30}]);
    value["nodes"][2]["boneId"] = json!("child");
    value["variables"] = json!({"enabled":true});
    value["clips"].as_array_mut().unwrap().push(json!({"id":"overlay","duration":2,"looping":true,"autoplay":true,"blendMode":"weighted","weight":0.5,"tracks":[{"nodeId":"ball","property":"y","keyframes":[{"time":0,"value":{"type":"number","value":0}},{"time":2,"value":{"type":"number","value":60},"easing":{"type":"cubicBezier","x1":0.42,"y1":0,"x2":0.58,"y2":1}}]},{"nodeId":"child","property":"rotation","keyframes":[{"time":0,"value":{"type":"number","value":20},"easing":{"type":"step"}}]}]}));
    // A valid silent PCM WAV fixture; no synthesis engine or fetch needed.
    value["audioAssets"] = json!([{"id":"voice","name":"Silent WAV fixture","mimeType":"audio/wav","dataBase64":"UklGRiYAAABXQVZFZm10IBAAAAABAAEAQB8AAIA+AAACABAAZGF0YQIAAAAAAA=="}]);
    value["audioCues"] = json!([{"id":"audio-cue","assetId":"voice","time":0,"offset":0,"duration":null,"volume":1}]);
    value["stateMachines"][0]["states"][1]["overrides"] =
        json!([{"nodeId":"ball","property":"opacity","value":{"type":"number","value":0.8}}]);
    value["stateMachines"][0]["transitions"][0]["guard"] =
        json!({"variable":"enabled","equals":true});
    value["stateMachines"][0]["transitions"][0]["actions"] = model()["actions"].clone();
    value["customJs"] = json!("/* Authored data only; review before --trusted-js. */");
    value["rawSvg"] = json!("<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
    project_value(&validated(value)?)
}

pub fn capabilities() -> Result<Value> {
    let mut command = crate::assist::command();
    command.build();
    Ok(json!({
        "format":"splinely-capabilities","capabilitiesVersion":1,
        "version":env!("CARGO_PKG_VERSION"),"schemaVersions":[1],
        "features":[
            {"id":"strict-project-json","commands":["new","apply","validate","schema","example"],"scope":"All canonical document fields can be authored directly or via merge patch; no GUI required"},
            {"id":"guided-authoring","commands":["guide","check","batch"],"scope":"Short workflow guide, lint with ready-to-run fixes, atomic multi-step edits"},
            {"id":"vector-geometry","commands":["node","import-svg","convert-to-path","convert-back","path-insert","path-handle","apply"],"kinds":["rect","circle","text","path","polygon","star","line"]},
            {"id":"style-transform-layers","commands":["node","arrange","layer","apply"],"scope":"Fill/stroke colours, linear/radial gradients, transforms, align/distribute/flip/scale/rotate, groups, stacking, boolean operations"},
            {"id":"animation","commands":["keyframe","clip","arrange morph","evaluate","render","apply"],"scope":"Number, colour, gradient and path (morph) tracks; linear/step/ease/cubic-bezier easing; looping/autoplay"},
            {"id":"accessibility","commands":["a11y","check","apply"],"scope":"Web-player/SVG screen-reader semantics: scene title/description, node label/description/role/hidden, state announcements, keyboard activation of interactive nodes; not verified with a real screen reader"},
            {"id":"interactions","commands":["on","apply"],"scope":"Events on shapes play/stop clips, set properties and switch states"},
            {"id":"weighted-blending","commands":["apply","blend-clip","evaluate"],"scope":"Override/weighted clips, per-property groups and blendClip actions"},
            {"id":"bones","commands":["apply","bone","rig-demo","keyframe"],"scope":"Hierarchy, node binding and local bone animation; authored rotations can represent IK poses (no runtime IK schema)"},
            {"id":"state-machines","commands":["on","apply"],"scope":"Parallel machines, states, guards, overrides, events and all actions; transitions crossfade with blend, several machines at once"},
            {"id":"motion-paths","commands":["motion-path","keyframe","apply"],"scope":"Optional node.motionPath binding; animatable motionProgress 0..1; tangent orientation; unbound M/L/Q/C/Z source paths"},
            {"id":"pointer-followers","commands":["follow","apply"],"scope":"Nodes or bones follow the pointer (offset or rotate) with max, strength, smoothing and optional state conditions; combine freely with clicks, hover and machines"},
            {"id":"variables","commands":["apply"],"scope":"Typed scalar variables"},
            {"id":"embedded-audio","commands":["apply","export"],"scope":"Base64 assets, global cues, playAudio"},
            {"id":"trusted-code-metadata","commands":["apply","export","plugin"],"scope":"customJs inert unless --trusted-js; rawSvg inert; native plugin scaffolds are editor-only source"},
            {"id":"drop-in-plugins","commands":["plugin list","plugin validate","plugin apply"],"scope":"Declarative editor part of drop-in plugins (addShape, setStyle, addBehavior: clip + state machine + variables + a11y label bound to a node); same candidate/validate path as the editor; nothing executed; native plugins are never loaded dynamically"},
            {"id":"runtime-plugins","commands":["export --plugin","isolated-preview --plugin"],"scope":"Opt-in browser runtime hooks (apiVersion 1: setup/onEvent/onState/onFrame) embedded hash-pinned only with explicit --trusted-js (isolated-preview: behind in-page consent); not a sandbox, lint is best-effort; never loaded from project data"},
            {"id":"showcases","commands":["new","water-drop","random-figures","rig-demo"]},
            {"id":"self-contained-export","commands":["export","inspect","evaluate"]}
        ],
        "commands":command.get_subcommands().map(command_info).collect::<Vec<_>>(),
        "model":model(),
        "examples":{"minimal":{"schemaVersion":1,"name":"Minimal","width":640,"height":360,"nodes":[]},"complete":complete_example()?},
        "protocol":{"success":"JSON stdout; help/version are text","failure":"Nonzero exit, JSON stderr {ok:false,error:{code,message,hint}}; hint names the next step","output":"Edit commands save back to INPUT when --output is omitted","stdin":"INPUT/PATCH -; only one stdin consumer","stdout":"Project --output - emits project; export --output - wraps SVG string in JSON","write":"Validate first, synced temporary in output directory then atomic replacement; serial writers; no automatic parent creation","patch":"RFC 7396: arrays replace entirely, null deletes members; input must already validate"},
        "limitations":["Schema guide is not a formal JSON Schema; validate is authoritative","No images, filters, masks or arbitrary SVG appearance in the canonical model (gradients and path morphing are supported)","Path edits accept only M/L/Q/C/Z and never approximate arcs or smooth commands","rawSvg is preserved, never injected or rendered","evaluate is deterministic core sampling, not browser playback; optional --context supplies states, ordered independent clip clocks, weights and final overrides; no JS, action execution, audio or runtime fade clock","Audio needs browser user gesture and supported codec","Trusted JavaScript and native code are unsandboxed; never run unreviewed code","SVG import (docs/svg-import.md) imports paths, text (one node per text/line, no font family), gradients and nested SVG data-URI images, reports everything approximated or dropped, drops raster images (no image node kind) and rejects unsupported appearance, external resources and malformed images; bundled runtime/export need no remote fetch","Hardware and GUI operations/history are not document fields or CLI features"],
        "guides":["LLM.md","CLI.md","BLENDING_SPEC.md","prompts/splinely-author.md"],
        "next":["splinely-cli guide","splinely-cli schema","splinely-cli --help","splinely-cli COMMAND --help","splinely-cli new --output project.json","splinely-cli apply project.json --patch authored.json --output project.json","splinely-cli validate project.json"]
    }))
}
