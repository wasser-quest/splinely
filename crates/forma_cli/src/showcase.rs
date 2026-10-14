//! Deterministic authoring helpers; all generated data passes the normal validator.
use crate::{failure, project_value, validated, Result};
use forma_document::{path_bounds, Color, Node, NodeKind, Project};
use serde_json::{json, Value};

fn painted(mut node: Node, color: [u8; 3]) -> Node {
    node.style.fill = Color::rgb(color[0], color[1], color[2]);
    node
}
fn text(id: &str, x: f64, y: f64, words: &str, size: f64, color: [u8; 3]) -> Node {
    let mut node = painted(Node::text(id, x, y, words), color);
    if let NodeKind::Text { font_size, .. } = &mut node.kind {
        *font_size = size;
    }
    node
}
fn disc(id: &str, x: f64, y: f64, radius: f64, color: [u8; 3]) -> Node {
    painted(Node::circle(id, x, y, radius), color)
}
fn scene(title: &str, subtitle: &str) -> Result<Value> {
    let mut nodes = vec![
        painted(Node::rect("background", 0., 0., 960., 640.), [12, 23, 43]),
        disc("ambient", 840., 180., 270., [16, 34, 57]),
        disc("ambient-inner", 840., 180., 210., [19, 40, 65]),
        text(
            "eyebrow",
            54.,
            66.,
            "FORMA / MOTION STUDIES",
            14.,
            [105, 196, 220],
        ),
        text("title", 54., 112., title, 34., [237, 246, 255]),
        text("subtitle", 54., 144., subtitle, 15., [156, 181, 203]),
    ];
    for (id, x, words) in [
        ("start-control", 54., "Play / resume"),
        ("pause-control", 222., "Pause"),
    ] {
        let mut button = painted(Node::rect(id, x, 567., 152., 40.), [32, 60, 84]);
        if let NodeKind::Rect { rx, .. } = &mut button.kind {
            *rx = 12.;
        }
        nodes.push(button);
        nodes.push(text(
            &format!("{id}-label"),
            x + 16.,
            593.,
            words,
            15.,
            [222, 240, 249],
        ));
    }
    nodes.push(text(
        "status",
        402.,
        593.,
        "Ready · use Play",
        14.,
        [156, 181, 203],
    ));
    project_value(&Project {
        name: title.into(),
        width: 960.,
        height: 640.,
        nodes,
        ..Project::default()
    })
}
fn add(value: &mut Value, mut node: Node) {
    if let NodeKind::Path { d, bounds, .. } = &mut node.kind {
        // Use the core's exact curve-extrema bounds, not control-point boxes.
        *bounds = path_bounds(d).expect("authoring helper constructs valid SVG paths");
    }
    value["nodes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::to_value(node).unwrap());
}
fn track(id: &str, property: &str, keys: &[(f64, f64)]) -> Value {
    json!({"nodeId":id,"property":property,"keyframes":keys.iter().map(|(time,value)| json!({
        "time":time,"value":{"type":"number","value":value},
        "easing":{"type":"cubicBezier","x1":0.42,"y1":0.0,"x2":0.58,"y2":1.0}
    })).collect::<Vec<_>>()})
}

// No timers, random state selection, private runtime fields, or implicit speech enable.
const CONTROLS: &str = r#"
const svg = document.currentScript.ownerSVGElement;
const data = JSON.parse(svg.querySelector('[id="forma-project"]').textContent);
const machine = data.stateMachines.length ? forma.machine('gallery') : null;
const status = svg.querySelector('[id="status"]');
let paused = [], pauseRequested = false;
function describe() {
  status.textContent = (machine ? 'State: ' + machine.state : 'Water drop') +
    ' · ' + (forma.timeline.playing ? 'playing' : 'paused') +
    ' · speech ' + (forma.speech.enabled ? 'enabled' : 'off');
}
function start() {
  pauseRequested = false;
  const host = svg.querySelector('[data-forma-host-control]');
  if (host) host.remove(); // These generated scenes contain no audio assets to unlock.
  if (machine && machine.state === 'idle') machine.send('START');
  else for (const id of paused) forma.clip(id).play();
  paused = []; forma.timeline.play(); describe();
}
function pause() {
  pauseRequested = true;
  if (forma.timeline.playing) {
    paused = data.clips.filter(c => forma.clip(c.id).playing).map(c => c.id);
    forma.timeline.pause();
  }
  describe();
}
for (const [id, fn] of [['start-control', start], ['pause-control', pause]]) {
  const el = svg.querySelector('[id="' + id + '"]');
  el.setAttribute('role', 'button'); el.setAttribute('tabindex', '0');
  el.setAttribute('aria-label', id === 'start-control' ? 'Play or resume animation' : 'Pause animation');
  el.style.cursor = 'pointer';
  const label = svg.querySelector('[id="' + id + '-label"]');
  label.style.pointerEvents = 'none';
  forma.on(id, 'click', fn);
  const key = event => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); fn(); } };
  el.addEventListener('keydown', key);
  forma.onDestroy(() => el.removeEventListener('keydown', key));
}
const host = svg.querySelector('[data-forma-host-control]');
if (host) {
  const key = event => { if (event.key === 'Enter' || event.key === ' ') start(); };
  host.addEventListener('click', start); host.addEventListener('keydown', key);
  forma.onDestroy(() => { host.removeEventListener('click', start); host.removeEventListener('keydown', key); });
}
forma.on('showcase.start', start);
forma.on('showcase.pause', pause);
forma.on('state.change', () => {
  // Pausing synchronizes the wall clock. A queued clip.end may enter the next
  // state after the Pause handler returns; hold that new clip, not the old one.
  if (pauseRequested) {
    paused = data.clips.filter(c => forma.clip(c.id).playing).map(c => c.id);
    forma.timeline.pause();
  }
  describe();
});
forma.on('error', () => { status.textContent = 'Runtime error · inspect diagnostics'; });
describe();
"#;

pub fn water_drop() -> Result<Project> {
    let mut value = scene(
        "A drop in the blue",
        "Squash, stretch, a soft landing. A deterministic 2.8-second loop.",
    )?;
    let mut floor = disc("floor-shadow", 0., 0., 160., [7, 15, 29]);
    floor.transform.x = 480.;
    floor.transform.y = 507.;
    floor.transform.scale_y = 0.14;
    add(&mut value, floor);
    // Concentric, subtly shifted circles approximate soft lighting using canonical geometry.
    for i in 0..48 {
        let t = i as f64 / 47.;
        add(
            &mut value,
            disc(
                &format!("ball-{i}"),
                480. - 25. * t,
                405. - 30. * t,
                120. - 66. * t,
                [
                    (27. + 36. * t) as u8,
                    (92. + 80. * t) as u8,
                    (197. + 40. * t) as u8,
                ],
            ),
        );
    }
    let mut shadow = disc("contact-shadow", 0., 0., 30., [18, 76, 145]);
    shadow.transform.x = 480.;
    shadow.transform.y = 289.;
    shadow.transform.scale_y = 0.18;
    shadow.style.opacity = 0.15;
    add(&mut value, shadow);
    let mut ripple = disc("ripple", 0., 0., 32., [110, 224, 247]);
    ripple.transform.x = 480.;
    ripple.transform.y = 287.;
    ripple.transform.scale_y = 0.18;
    ripple.style.fill.a = 0.;
    ripple.style.stroke = Some(Color::rgb(139, 235, 255));
    ripple.style.stroke_width = 2.;
    ripple.style.opacity = 0.;
    add(&mut value, ripple);
    let mut drop = painted(
        Node::new(
            "drop",
            NodeKind::Path {
                d: "M 0 -60 C -8 -35 -31 -10 -30 8 C -29 40 29 40 30 8 C 31 -10 8 -35 0 -60 Z"
                    .into(),
                bounds: [-31., -60., 62., 100.],
                evenodd: false,
            },
        ),
        [103, 226, 245],
    );
    drop.transform.x = 480.;
    drop.transform.y = 130.;
    add(&mut value, drop);
    let mut glint = painted(
        Node::new(
            "drop-glint",
            NodeKind::Path {
                d: "M -9 -17 C -22 -1 -23 13 -13 20 C -18 5 -10 -6 -9 -17 Z".into(),
                bounds: [-23., -17., 14., 37.],
                evenodd: false,
            },
        ),
        [221, 255, 255],
    );
    glint.transform.x = 480.;
    glint.transform.y = 130.;
    add(&mut value, glint);
    let mut tracks = Vec::new();
    for id in ["drop", "drop-glint"] {
        tracks.push(track(
            id,
            "y",
            &[
                (0., 130.),
                (0.9, 247.),
                (1.02, 260.),
                (1.16, 242.),
                (1.8, 130.),
                (2.8, 130.),
            ],
        ));
        tracks.push(track(
            id,
            "scaleX",
            &[
                (0., 1.),
                (0.9, 0.84),
                (1.02, 1.32),
                (1.16, 0.86),
                (1.8, 1.),
                (2.8, 1.),
            ],
        ));
        tracks.push(track(
            id,
            "scaleY",
            &[
                (0., 1.),
                (0.9, 1.16),
                (1.02, 0.62),
                (1.16, 1.14),
                (1.8, 1.),
                (2.8, 1.),
            ],
        ));
    }
    tracks.push(track(
        "contact-shadow",
        "opacity",
        &[(0., 0.12), (1.02, 0.65), (1.8, 0.12), (2.8, 0.12)],
    ));
    tracks.push(track(
        "contact-shadow",
        "scaleX",
        &[(0., 0.6), (1.02, 1.3), (1.8, 0.6), (2.8, 0.6)],
    ));
    tracks.push(track(
        "ripple",
        "opacity",
        &[(0., 0.), (0.98, 0.), (1.04, 0.8), (1.7, 0.), (2.8, 0.)],
    ));
    tracks.push(track(
        "ripple",
        "scaleX",
        &[(0., 0.6), (0.98, 0.6), (1.7, 2.8), (2.8, 2.8)],
    ));
    value["clips"] = json!([{"id":"bounce","name":"Water drop bounce","duration":2.8,"looping":true,"autoplay":true,"tracks":tracks}]);
    value["customJs"] = json!(CONTROLS);
    validated(value)
}
// Explicit wrapping LCG: reproducible across platforms, with no rand dependency.
struct Seed(u64);
impl Seed {
    fn unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / ((1u64 << 53) as f64)
    }
}

pub fn random_figures(seed: u64, count: usize) -> Result<Project> {
    if !(3..=12).contains(&count) {
        return Err(failure(
            "invalid_arguments",
            "count must be between 3 and 12",
        ));
    }
    let mut rng = Seed(seed);
    let mut value = scene(
        "The shape conversation",
        "Seeded geometry. Every state gets its turn; narration is optional.",
    )?;
    let mut stage = disc("stage", 0., 0., 170., [23, 43, 66]);
    stage.transform.x = 480.;
    stage.transform.y = 328.;
    stage.transform.scale_y = 0.87;
    add(&mut value, stage);
    let palette = [
        [102, 224, 218],
        [255, 184, 107],
        [172, 153, 255],
        [255, 135, 164],
        [144, 207, 255],
        [207, 232, 147],
    ];
    let phrases = [
        "Hello! I am a curious circle.",
        "I am a soft square. Let's build something.",
        "I am a bright polygon. Every corner has a story.",
    ];
    let mut states = vec![json!({"id":"idle","onEnter":[],"overrides":[]})];
    let mut transitions =
        vec![json!({"from":"idle","to":"figure-0","event":{"name":"START"},"actions":[]})];
    let mut clips = Vec::new();
    for i in 0..count {
        let id = format!("shape-{i}");
        let label = format!("phrase-{i}");
        let kind = (rng.unit() * 3.) as usize;
        let size = 75. + rng.unit() * 30.;
        let mut node = match kind {
            0 => Node::circle(&id, 0., 0., size),
            1 => {
                let mut n = Node::rect(&id, -size, -size, 2. * size, 2. * size);
                if let NodeKind::Rect { rx, .. } = &mut n.kind {
                    *rx = 26.;
                }
                n
            }
            _ => {
                let sides = 3 + (rng.unit() * 5.) as usize;
                let mut d = String::new();
                for j in 0..sides {
                    let angle = j as f64 * std::f64::consts::TAU / sides as f64
                        - std::f64::consts::FRAC_PI_2;
                    d.push_str(&format!(
                        "{} {:.4} {:.4} ",
                        if j == 0 { "M" } else { "L" },
                        size * angle.cos(),
                        size * angle.sin()
                    ));
                }
                d.push('Z');
                Node::new(
                    &id,
                    NodeKind::Path {
                        d,
                        bounds: [-size, -size, size * 2., size * 2.],
                        evenodd: false,
                    },
                )
            }
        };
        node.style.fill = Color::rgb(palette[i % 6][0], palette[i % 6][1], palette[i % 6][2]);
        node.style.opacity = 0.;
        node.transform.x = 480.;
        node.transform.y = 320.;
        add(&mut value, node);
        let mut caption = text(&label, 180., 506., phrases[kind], 20., [222, 237, 248]);
        caption.style.opacity = 0.;
        add(&mut value, caption);
        let motion = format!("motion-{i}");
        let sway = 12. + rng.unit() * 20.;
        clips.push(
            json!({"id":motion,"duration":4.8,"looping":false,"autoplay":false,"tracks":[
                track(&id,"scaleX",&[(0.,0.65),(0.65,1.08),(1.,1.),(3.8,1.),(4.8,0.65)]),
                track(&id,"scaleY",&[(0.,0.65),(0.65,1.08),(1.,1.),(3.8,1.),(4.8,0.65)]),
                track(&id,"y",&[(0.,350.),(1.,320.),(2.4,305.),(3.8,320.),(4.8,350.)]),
                track(&id,"rotation",&[(0.,-sway),(1.,0.),(2.4,sway),(3.8,0.),(4.8,-sway)]),
                track(&id,"opacity",&[(0.,0.),(0.55,1.),(4.1,1.),(4.8,0.)])
            ]}),
        );
        states.push(json!({"id":format!("figure-{i}"),"onEnter":[
            {"type":"playClip","clipId":motion},
            {"type":"speak","text":phrases[kind],"lang":"en-US"}
        ],"overrides":[
            {"nodeId":id,"property":"opacity","value":{"type":"number","value":1.}},
            {"nodeId":label,"property":"opacity","value":{"type":"number","value":1.}}
        ]}));
        transitions.push(
            json!({"from":format!("figure-{i}"),"to":format!("figure-{}",(i+1)%count),
            "event":{"name":"clip.end"},"actions":[{"type":"stopClip","clipId":motion}]}),
        );
    }
    value["clips"] = json!(clips);
    value["stateMachines"] =
        json!([{"id":"gallery","initial":"idle","states":states,"transitions":transitions}]);
    value["variables"] = json!({"seed":seed.to_string(),"figureCount":count});
    value["customJs"] = json!(CONTROLS);
    validated(value)
}
