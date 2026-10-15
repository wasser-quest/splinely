//! Maps a resolved usvg gradient onto the project's bounding-box gradient model
//! (`Gradient`: angle or centred radial, 2..16 stops, pad spread).
//! usvg has already resolved `href` inheritance, `gradientUnits` and `gradientTransform`;
//! what the model cannot express is reported as a note, never dropped silently.
use crate::{Color, Gradient, GradientStop};
use usvg::tiny_skia_path::Transform;

pub(crate) enum Mapped {
    NotGradient,
    Gradient(Gradient),
    /// The gradient degenerates to one colour (zero-length vector, empty box).
    Solid(Color),
}

/// `(code, message)` pairs for approximations.
pub(crate) type Notes = Vec<(&'static str, String)>;

const EPS: f64 = 1e-9;

fn apply(t: &Transform, x: f64, y: f64) -> (f64, f64) {
    (
        t.sx as f64 * x + t.kx as f64 * y + t.tx as f64,
        t.ky as f64 * x + t.sy as f64 * y + t.ty as f64,
    )
}

fn lerp(a: Color, b: Color, t: f64) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

fn color_at(stops: &[GradientStop], o: f64) -> Color {
    let (first, last) = (stops[0], stops[stops.len() - 1]);
    if o <= first.offset {
        return first.color;
    }
    if o >= last.offset {
        return last.color;
    }
    for w in stops.windows(2) {
        if o >= w[0].offset && o < w[1].offset {
            let span = w[1].offset - w[0].offset;
            return lerp(w[0].color, w[1].color, (o - w[0].offset) / span);
        }
    }
    last.color
}

/// Re-express stops under `m = a * o + b` (model offset from source offset),
/// padding outside the source range like SVG's default `pad`.
fn remap(stops: &[GradientStop], a: f64, b: f64, notes: &mut Notes) -> Vec<GradientStop> {
    let o_of = |m: f64| (m - b) / a;
    let in_range = |s: &GradientStop| (-EPS..=1. + EPS).contains(&(a * s.offset + b));
    if !stops.iter().all(in_range) {
        notes.push((
            "gradient-range",
            "the gradient vector reaches beyond the model's fixed-length gradient line; stops outside it were dropped and the end colours re-interpolated".into(),
        ));
    }
    let mut out: Vec<GradientStop> = stops
        .iter()
        .filter_map(|s| {
            let m = a * s.offset + b;
            (-EPS..=1. + EPS).contains(&m).then(|| GradientStop {
                offset: ((m.clamp(0., 1.)) * 1e6).round() / 1e6,
                color: s.color,
            })
        })
        .collect();
    if out.first().is_none_or(|s| s.offset > 1e-6) {
        out.insert(
            0,
            GradientStop {
                offset: 0.,
                color: color_at(stops, o_of(0.)),
            },
        );
    }
    if out.last().is_none_or(|s| s.offset < 1. - 1e-6) {
        out.push(GradientStop {
            offset: 1.,
            color: color_at(stops, o_of(1.)),
        });
    }
    if out.len() > 16 {
        let n = out.len();
        notes.push((
            "gradient-stops",
            format!("{n} stops resampled to the model maximum of 16 (nearest stops kept)"),
        ));
        out = (0..16).map(|i| out[(i * (n - 1) + 7) / 15]).collect();
    }
    out
}

pub(crate) fn map_paint(
    paint: &usvg::Paint,
    opacity: f32,
    to_space: Transform,
    bbox: [f64; 4],
    notes: &mut Notes,
) -> Mapped {
    let (base, radial): (&usvg::BaseGradient, Option<&usvg::RadialGradient>) = match paint {
        usvg::Paint::LinearGradient(g) => (g, None),
        usvg::Paint::RadialGradient(g) => (g, Some(g)),
        _ => return Mapped::NotGradient,
    };
    let stops: Vec<GradientStop> = base
        .stops()
        .iter()
        .map(|stop| {
            let c = stop.color();
            let mut color = Color::rgb(c.red, c.green, c.blue);
            color.a = (stop.opacity().get() * opacity) as f64;
            GradientStop {
                offset: stop.offset().get() as f64,
                color,
            }
        })
        .collect();
    let Some(last) = stops.last().copied() else {
        return Mapped::Solid(Color::rgb(0, 0, 0));
    };
    let [bx, by, bw, bh] = bbox;
    if bw <= EPS || bh <= EPS {
        notes.push((
            "gradient-degenerate",
            "shape has an empty bounding box; the gradient became its last stop colour".into(),
        ));
        return Mapped::Solid(last.color);
    }
    if base.spread_method() != usvg::SpreadMethod::Pad {
        notes.push((
            "gradient-spread",
            "spreadMethod reflect/repeat is not supported; treated as pad".into(),
        ));
    }
    let to_box = to_space.pre_concat(base.transform());
    let norm = |p: (f64, f64)| ((p.0 - bx) / bw, (p.1 - by) / bh);
    let lin = |v: (f64, f64)| {
        (
            to_box.sx as f64 * v.0 + to_box.kx as f64 * v.1,
            to_box.ky as f64 * v.0 + to_box.sy as f64 * v.1,
        )
    };
    let finish = |radial, angle: f64, stops: Vec<GradientStop>| {
        Mapped::Gradient(Gradient {
            radial,
            angle: (angle * 1000.).round() / 1000. + 0.,
            stops,
        })
    };
    match radial {
        None => {
            // Only the Deref'd linear fields are needed; fetch them through the paint.
            let usvg::Paint::LinearGradient(g) = paint else {
                unreachable!()
            };
            let (x1, y1, x2, y2) = (g.x1() as f64, g.y1() as f64, g.x2() as f64, g.y2() as f64);
            let n1 = norm(apply(&to_box, x1, y1));
            let n2 = norm(apply(&to_box, x2, y2));
            let d = (n2.0 - n1.0, n2.1 - n1.1);
            let len = d.0.hypot(d.1);
            if len < EPS {
                notes.push((
                    "gradient-degenerate",
                    "zero-length gradient vector; became the last stop colour".into(),
                ));
                return Mapped::Solid(last.color);
            }
            let angle = d.1.atan2(d.0).to_degrees();
            let (sin, cos) = ((angle * 1000.).round() / 1000.).to_radians().sin_cos();
            let s = (0.5 - cos / 2., 0.5 - sin / 2.);
            let o0 = ((s.0 - n1.0) * d.0 + (s.1 - n1.1) * d.1) / (len * len);
            let out = remap(&stops, len, -o0 * len, notes);
            // Isolines must stay perpendicular in the box; the model's are perpendicular
            // in normalised box space.
            let src = lin((-(y2 - y1), x2 - x1));
            let model = (-d.1 * bw, d.0 * bh);
            let cross = src.0 * model.1 - src.1 * model.0;
            if cross.abs() > 1e-4 * src.0.hypot(src.1) * model.0.hypot(model.1) {
                notes.push((
                    "gradient-skew",
                    "gradient bands are skewed (non-square box or gradientTransform); the model keeps them perpendicular in the bounding box".into(),
                ));
            }
            finish(false, angle, out)
        }
        Some(g) => {
            let r = g.r().get() as f64;
            let c = norm(apply(&to_box, g.cx() as f64, g.cy() as f64));
            let ex = r * ((to_box.sx as f64).hypot(to_box.kx as f64));
            let ey = r * ((to_box.ky as f64).hypot(to_box.sy as f64));
            if ex < EPS || ey < EPS {
                notes.push((
                    "gradient-degenerate",
                    "radial gradient has zero radius; became the last stop colour".into(),
                ));
                return Mapped::Solid(last.color);
            }
            let cov = to_box.sx as f64 * to_box.ky as f64 + to_box.kx as f64 * to_box.sy as f64;
            if cov.abs() > 1e-4 * ex * ey / (r * r) {
                notes.push((
                    "gradient-skew",
                    "radial gradient is rotated or skewed; treated as axis-aligned".into(),
                ));
            }
            let (rnx, rny) = (ex / bw, ey / bh);
            let rho = if (rnx - rny).abs() <= 1e-4 * rnx.max(rny) {
                rnx
            } else {
                notes.push((
                    "gradient-radius",
                    "radial gradient radius differs along width and height; the average is used"
                        .into(),
                ));
                (rnx + rny) / 2.
            };
            if (c.0 - 0.5).abs() > 1e-4 || (c.1 - 0.5).abs() > 1e-4 {
                notes.push((
                    "gradient-center",
                    "radial gradient centre moved to the middle of the bounding box".into(),
                ));
            }
            let focal = (g.fx() - g.cx()).abs().max((g.fy() - g.cy()).abs()) as f64;
            if focal > 1e-4 * r.max(1e-9) {
                notes.push((
                    "gradient-focal",
                    "focal point (fx/fy) is not supported; the centre is used".into(),
                ));
            }
            finish(true, 0., remap(&stops, 2. * rho, 0., notes))
        }
    }
}
