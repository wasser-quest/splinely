use crate::ecs::{SceneNode, Shape};
use forma_document::{Color, Node, NodeKind};
fn color(c: &Color) -> String {
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        (c.r * 255.) as u8,
        (c.g * 255.) as u8,
        (c.b * 255.) as u8,
        (c.a * 255.) as u8
    )
}
pub fn project_node(n: &Node) -> Option<SceneNode> {
    if !n.visible {
        return None;
    }
    let (x, y, shape) = match &n.kind {
        NodeKind::Path { d, bounds, .. } => (
            0.,
            0.,
            Shape::Path {
                d: d.clone(),
                bounds: bounds.map(|v| v as f32),
                evenodd: matches!(n.kind, NodeKind::Path { evenodd: true, .. }),
            },
        ),
        NodeKind::Rect {
            x,
            y,
            width,
            height,
            rx,
        } => (
            *x,
            *y,
            Shape::Rect {
                width: *width as f32,
                height: *height as f32,
                rx: *rx as f32,
            },
        ),
        NodeKind::Circle { cx, cy, radius } => (
            *cx,
            *cy,
            Shape::Circle {
                radius: *radius as f32,
            },
        ),
        NodeKind::Text {
            x,
            y,
            text,
            font_size,
        } => (
            *x,
            *y,
            Shape::Text {
                text: text.clone(),
                size: *font_size as f32,
            },
        ),
    };
    // Geometry is transformed around SVG origin, just like exported transform lists.
    let a = n.transform.rotation.to_radians();
    let gx = x * n.transform.scale_x;
    let gy = y * n.transform.scale_y;
    Some(SceneNode {
        id: n.id.clone(),
        x: (n.transform.x + gx * a.cos() - gy * a.sin()) as f32,
        y: (n.transform.y + gx * a.sin() + gy * a.cos()) as f32,
        rotation: n.transform.rotation as f32,
        scale_x: n.transform.scale_x as f32,
        scale_y: n.transform.scale_y as f32,
        opacity: n.style.opacity as f32,
        fill: color(&n.style.fill),
        gradient: n.gradient.clone(),
        stroke: n
            .style
            .stroke
            .as_ref()
            .map(color)
            .unwrap_or_else(|| "none".into()),
        stroke_width: n.style.stroke_width as f32,
        shape,
    })
}
