//! Explicit path edits; never approximate unsupported SVG commands.
use crate::{failure, load, save_project, Result};
use clap::ValueEnum;
use forma_document::NodeKind;
use serde_json::Value;
use svgtypes::{PathParser, PathSegment};

type Point = [f64; 2];
#[derive(Clone, Debug, PartialEq)]
enum Command {
    Move(Point),
    Line(Point),
    Quad(Point, Point),
    Cubic(Point, Point, Point),
    Close,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Handle {
    Control1,
    Control2,
    Anchor,
}

fn parse(d: &str) -> Result<Vec<Command>> {
    let mut result = Vec::new();
    let mut current = [0.0, 0.0];
    let mut start = current;
    for segment in PathParser::from(d) {
        let segment = segment.map_err(|e| failure("invalid_path", e))?;
        let point = |abs: bool, x: f64, y: f64| {
            if abs {
                [x, y]
            } else {
                [current[0] + x, current[1] + y]
            }
        };
        let command = match segment {
            PathSegment::MoveTo { abs, x, y } => Command::Move(point(abs, x, y)),
            PathSegment::LineTo { abs, x, y } => Command::Line(point(abs, x, y)),
            PathSegment::Quadratic { abs, x1, y1, x, y } => Command::Quad(point(abs, x1, y1), point(abs, x, y)),
            PathSegment::CurveTo { abs, x1, y1, x2, y2, x, y } => Command::Cubic(point(abs, x1, y1), point(abs, x2, y2), point(abs, x, y)),
            PathSegment::ClosePath { .. } => Command::Close,
            _ => return Err(failure("unsupported_path", "Path editing supports only M/L/Q/C/Z (absolute or relative); H/V/S/T/A must be normalized explicitly first")),
        };
        match command {
            Command::Move(p) => {
                current = p;
                start = p;
            }
            Command::Line(p) | Command::Quad(_, p) | Command::Cubic(_, _, p) => current = p,
            Command::Close => current = start,
        }
        result.push(command);
    }
    Ok(result)
}

fn encode(commands: &[Command]) -> String {
    commands
        .iter()
        .map(|command| match command {
            Command::Move([x, y]) => format!("M {x} {y}"),
            Command::Line([x, y]) => format!("L {x} {y}"),
            Command::Quad([a, b], [x, y]) => format!("Q {a} {b} {x} {y}"),
            Command::Cubic([a, b], [c, d], [x, y]) => format!("C {a} {b} {c} {d} {x} {y}"),
            Command::Close => "Z".into(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn split(commands: &mut Vec<Command>, segment: usize, t: f64) -> Result<()> {
    if !t.is_finite() || t <= 0.0 || t >= 1.0 {
        return Err(failure(
            "invalid_arguments",
            "t must be finite and strictly between 0 and 1",
        ));
    }
    let mut current = [0.0, 0.0];
    let mut start = current;
    let mut index = 0;
    for i in 0..commands.len() {
        let command = commands[i].clone();
        if matches!(
            command,
            Command::Line(_) | Command::Quad(_, _) | Command::Cubic(_, _, _)
        ) {
            if index == segment {
                let halves = match command {
                    Command::Line(p) => vec![Command::Line(lerp(current, p, t)), Command::Line(p)],
                    Command::Quad(c, p) => {
                        let a = lerp(current, c, t);
                        let b = lerp(c, p, t);
                        let mid = lerp(a, b, t);
                        vec![Command::Quad(a, mid), Command::Quad(b, p)]
                    }
                    Command::Cubic(c1, c2, p) => {
                        let a = lerp(current, c1, t);
                        let b = lerp(c1, c2, t);
                        let c = lerp(c2, p, t);
                        let d = lerp(a, b, t);
                        let e = lerp(b, c, t);
                        let mid = lerp(d, e, t);
                        vec![Command::Cubic(a, d, mid), Command::Cubic(e, c, p)]
                    }
                    _ => unreachable!(),
                };
                commands.splice(i..=i, halves);
                return Ok(());
            }
            index += 1;
        }
        match command {
            Command::Move(p) => {
                current = p;
                start = p;
            }
            Command::Line(p) | Command::Quad(_, p) | Command::Cubic(_, _, p) => current = p,
            Command::Close => current = start,
        }
    }
    Err(failure(
        "not_found",
        "Segment index out of range (counts L/Q/C only, not M/Z)",
    ))
}

fn edit(
    input: &str,
    output: &str,
    id: &str,
    operation: impl FnOnce(&mut Vec<Command>) -> Result<()>,
) -> Result<Value> {
    let mut project = load(input)?;
    let ids: Vec<String> = project.nodes.iter().map(|n| n.id.clone()).collect();
    let node = project
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .ok_or_else(|| crate::assist::not_found("node", id, ids.iter().map(String::as_str)))?;
    let NodeKind::Path { d, bounds, .. } = &mut node.kind else {
        return Err(failure(
            "invalid_arguments",
            "Node must be a path; use convert-to-path first",
        ));
    };
    let mut commands = parse(d)?;
    operation(&mut commands)?;
    let updated = encode(&commands);
    *bounds = forma_document::path_bounds(&updated).map_err(|e| failure("invalid_path", e))?;
    *d = updated;
    save_project(&project, output)
}

pub fn insert(input: &str, output: &str, id: &str, segment: usize, t: f64) -> Result<Value> {
    edit(input, output, id, |commands| split(commands, segment, t))
}

pub fn handle(
    input: &str,
    output: &str,
    id: &str,
    index: usize,
    handle: Handle,
    x: f64,
    y: f64,
) -> Result<Value> {
    if !x.is_finite() || !y.is_finite() {
        return Err(failure(
            "invalid_arguments",
            "Handle coordinates must be finite",
        ));
    }
    edit(input, output, id, |commands| {
        let command = commands
            .get_mut(index)
            .ok_or_else(|| failure("not_found", "Command index out of range"))?;
        let point = match (command, handle) {
            (
                Command::Move(p) | Command::Line(p) | Command::Quad(_, p) | Command::Cubic(_, _, p),
                Handle::Anchor,
            ) => p,
            (Command::Quad(p, _) | Command::Cubic(p, _, _), Handle::Control1) => p,
            (Command::Cubic(_, p, _), Handle::Control2) => p,
            _ => {
                return Err(failure(
                    "invalid_arguments",
                    "Selected command has no such handle; Z has none, Q has control1 only",
                ))
            }
        };
        *point = [x, y];
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(start: Point, command: &Command, t: f64) -> Point {
        match *command {
            Command::Line(p) => lerp(start, p, t),
            Command::Quad(c, p) => lerp(lerp(start, c, t), lerp(c, p, t), t),
            Command::Cubic(a, b, p) => lerp(
                lerp(lerp(start, a, t), lerp(a, b, t), t),
                lerp(lerp(a, b, t), lerp(b, p, t), t),
                t,
            ),
            _ => panic!("not drawable"),
        }
    }
    #[test]
    fn splits_preserve_shape_at_non_midpoint() {
        for original in [
            Command::Line([90., 40.]),
            Command::Quad([30., 100.], [90., 40.]),
            Command::Cubic([10., 100.], [70., -80.], [90., 40.]),
        ] {
            let start = [5., 7.];
            let mut commands = vec![Command::Move(start), original.clone()];
            let t = 0.37;
            split(&mut commands, 0, t).unwrap();
            let mid = sample(start, &commands[1], 1.);
            for i in 0..=100 {
                let u = i as f64 / 100.;
                let expected = sample(start, &original, u);
                let actual = if u <= t {
                    sample(start, &commands[1], u / t)
                } else {
                    sample(mid, &commands[2], (u - t) / (1. - t))
                };
                for axis in 0..2 {
                    assert!((expected[axis] - actual[axis]).abs() < 1e-10);
                }
            }
        }
    }
    #[test]
    fn relative_and_subpaths_are_normalized_without_losing_close() {
        let mut commands = parse("m 10 20 q 10 20 30 0 z m 5 0 l 10 0").unwrap();
        split(&mut commands, 1, 0.5).unwrap();
        assert_eq!(commands[3], Command::Move([15., 20.]));
        assert_eq!(commands[4], Command::Line([20., 20.]));
        assert_eq!(commands[5], Command::Line([25., 20.]));
        assert!(parse("M0 0 H10").is_err());
        assert!(split(&mut commands, 9, 0.5).is_err());
    }
}
