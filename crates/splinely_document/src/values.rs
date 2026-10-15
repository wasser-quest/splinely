//! Structured animation values as "shape + numbers": numbers, colours,
//! gradients (angle and stops) and path data (command letters and numbers)
//! all interpolate and blend number by number when their shapes match.
use crate::*;

/// One token of SVG path data.
#[derive(Clone, Debug, PartialEq)]
enum Token {
    Command(char),
    Number(f64),
}

fn tokenize(d: &str) -> Option<Vec<Token>> {
    let mut tokens = vec![];
    let bytes = d.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_whitespace() || c == ',' {
            i += 1;
        } else if c.is_ascii_alphabetic() {
            tokens.push(Token::Command(c));
            i += 1;
        } else {
            let start = i;
            if matches!(bytes[i], b'+' | b'-') {
                i += 1;
            }
            let mut dot = false;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || (bytes[i] == b'.' && !dot)) {
                dot |= bytes[i] == b'.';
                i += 1;
            }
            if i < bytes.len() && matches!(bytes[i], b'e' | b'E') {
                i += 1;
                if i < bytes.len() && matches!(bytes[i], b'+' | b'-') {
                    i += 1;
                }
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let number: f64 = d.get(start..i)?.parse().ok()?;
            tokens.push(Token::Number(number));
        }
    }
    Some(tokens)
}

/// Format a path number compactly (3 decimals, no "-0").
pub(crate) fn path_number(v: f64) -> String {
    let r = (v * 1000.).round() / 1000. + 0.;
    format!("{r}")
}

/// Structure signature of path data: command letters and number counts.
fn path_shape(d: &str) -> Option<String> {
    let mut shape = String::new();
    for token in tokenize(d)? {
        match token {
            Token::Command(c) => shape.push(c),
            Token::Number(_) => shape.push('#'),
        }
    }
    Some(shape)
}

fn path_numbers(d: &str) -> Vec<f64> {
    tokenize(d)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|t| match t {
            Token::Number(n) => Some(n),
            Token::Command(_) => None,
        })
        .collect()
}

fn rebuild_path(template: &str, numbers: &[f64]) -> String {
    let mut out = String::new();
    let mut next = numbers.iter();
    for token in tokenize(template).unwrap_or_default() {
        match token {
            Token::Command(c) => {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push(c);
            }
            Token::Number(_) => {
                if !out.ends_with(|c: char| c.is_ascii_alphabetic()) {
                    out.push(' ');
                }
                out.push_str(&path_number(*next.next().unwrap_or(&0.)));
            }
        }
    }
    out
}

impl AnimValue {
    /// Whether two values can be interpolated number by number.
    pub fn compatible(&self, other: &AnimValue) -> bool {
        match (self, other) {
            (AnimValue::Number(_), AnimValue::Number(_)) => true,
            (AnimValue::Color(_), AnimValue::Color(_)) => true,
            (AnimValue::Gradient(a), AnimValue::Gradient(b)) => {
                a.radial == b.radial && a.stops.len() == b.stops.len()
            }
            (AnimValue::Path(a), AnimValue::Path(b)) => {
                path_shape(a).is_some() && path_shape(a) == path_shape(b)
            }
            _ => false,
        }
    }

    /// The value's numbers in a fixed order.
    pub(crate) fn numbers(&self) -> Vec<f64> {
        match self {
            AnimValue::Number(v) => vec![*v],
            AnimValue::Color(c) => vec![c.r, c.g, c.b, c.a],
            AnimValue::Gradient(g) => {
                let mut out = vec![g.angle];
                for s in &g.stops {
                    out.extend([s.offset, s.color.r, s.color.g, s.color.b, s.color.a]);
                }
                out
            }
            AnimValue::Path(d) => path_numbers(d),
        }
    }

    /// A value shaped like `self` carrying `numbers`.
    pub(crate) fn with_numbers(&self, n: &[f64]) -> AnimValue {
        let color = |i: usize| Color {
            r: n[i],
            g: n[i + 1],
            b: n[i + 2],
            a: n[i + 3],
        };
        match self {
            AnimValue::Number(_) => AnimValue::Number(n[0]),
            AnimValue::Color(_) => AnimValue::Color(color(0)),
            AnimValue::Gradient(g) => AnimValue::Gradient(Gradient {
                radial: g.radial,
                angle: n[0],
                stops: (0..g.stops.len())
                    .map(|i| GradientStop {
                        offset: n[1 + i * 5].clamp(0., 1.),
                        color: color(2 + i * 5),
                    })
                    .collect(),
            }),
            AnimValue::Path(d) => AnimValue::Path(rebuild_path(d, n)),
        }
    }

    /// Linear interpolation; incompatible values switch at the end (step).
    pub fn interpolate(&self, other: &AnimValue, t: f64) -> AnimValue {
        if !self.compatible(other) {
            return if t < 1. { self.clone() } else { other.clone() };
        }
        let (a, b) = (self.numbers(), other.numbers());
        let mixed: Vec<f64> = a
            .iter()
            .zip(&b)
            .map(|(a, b)| a * (1. - t) + b * t)
            .collect();
        self.with_numbers(&mixed)
    }
}

/// Check that a track's keyframes can be interpolated with each other.
pub(crate) fn track_compatible(track: &Track) -> bool {
    track
        .keyframes
        .windows(2)
        .all(|w| w[0].value.compatible(&w[1].value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_interpolate_number_by_number() {
        let a = AnimValue::Path("M0 0 L10 0 C10 10 0 10 0 0 Z".into());
        let b = AnimValue::Path("M10,10 L30 10 C 30 30 10 30 10 10z".into());
        assert!(!a.compatible(&b), "z vs Z differs");
        let b = AnimValue::Path("M10,10 L30 10 C 30 30 10 30 10 10Z".into());
        assert!(a.compatible(&b));
        assert_eq!(
            a.interpolate(&b, 0.5),
            AnimValue::Path("M5 5 L20 5 C20 20 5 20 5 5 Z".into())
        );
        assert_eq!(path_numbers("M-1.5e1-.5"), vec![-15., -0.5]);
        let other = AnimValue::Path("M0 0 L1 1".into());
        assert_eq!(a.interpolate(&other, 0.5), a, "incompatible: step");
    }

    #[test]
    fn gradients_interpolate_angle_and_stops() {
        let a = AnimValue::Gradient(Gradient::linear(
            0.,
            Color::rgb(0, 0, 0),
            Color::rgb(255, 255, 255),
        ));
        let b = AnimValue::Gradient(Gradient::linear(
            90.,
            Color::rgb(255, 255, 255),
            Color::rgb(0, 0, 0),
        ));
        let AnimValue::Gradient(mid) = a.interpolate(&b, 0.5) else {
            panic!()
        };
        assert_eq!(mid.angle, 45.);
        assert!((mid.stops[0].color.r - 0.5).abs() < 1e-9);
        let mut radial = Gradient::linear(0., Color::rgb(0, 0, 0), Color::rgb(0, 0, 0));
        radial.radial = true;
        assert!(!a.compatible(&AnimValue::Gradient(radial)));
    }
}
