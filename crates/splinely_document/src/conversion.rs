use crate::{DocumentError, Node, NodeKind};

/// Deterministic exact SVG geometry (arcs, not Bezier approximations).
pub(crate) fn generated_path(kind: &NodeKind) -> Result<NodeKind, DocumentError> {
    let (d, bounds) = match *kind {
        NodeKind::Rect {
            x,
            y,
            width,
            height,
            rx,
        } => {
            if ![x, y, width, height, rx].iter().all(|v| v.is_finite())
                || width <= 0.0
                || height <= 0.0
                || rx < 0.0
            {
                return Err(DocumentError(
                    "conversion requires finite, positive rect dimensions and nonnegative rx"
                        .into(),
                ));
            }
            let (right, bottom) = (x + width, y + height);
            let (a, b) = (rx.min(width / 2.0), rx.min(height / 2.0));
            let d = if rx == 0.0 {
                format!("M{x} {y} H{right} V{bottom} H{x} Z")
            } else {
                format!("M{} {y} H{} A{a} {b} 0 0 1 {right} {} V{} A{a} {b} 0 0 1 {} {bottom} H{} A{a} {b} 0 0 1 {x} {} V{} A{a} {b} 0 0 1 {} {y} Z",
                    x+a, right-a, y+b, bottom-b, right-a, x+a, bottom-b, y+b, x+a)
            };
            (d, [x, y, width, height])
        }
        NodeKind::Circle { cx, cy, radius } => {
            if ![cx, cy, radius].iter().all(|v| v.is_finite()) || radius <= 0.0 {
                return Err(DocumentError(
                    "conversion requires a finite, positive circle radius".into(),
                ));
            }
            let (left, right) = (cx - radius, cx + radius);
            (format!("M{right} {cy} A{radius} {radius} 0 1 1 {left} {cy} A{radius} {radius} 0 1 1 {right} {cy} Z"),
                [left, cy-radius, radius*2.0, radius*2.0])
        }
        _ => {
            return Err(DocumentError(
                "only rect and circle support path conversion; text requires font outlines".into(),
            ))
        }
    };
    if !bounds.iter().all(|v| v.is_finite()) {
        return Err(DocumentError("conversion geometry overflow".into()));
    }
    crate::svg_import::validate_path(&d)?;
    Ok(NodeKind::Path {
        d,
        bounds,
        evenodd: false,
    })
}

impl Node {
    /// Preserve local geometry, style, transform, binding and UI flags.
    pub fn convert_to_path(&mut self) -> Result<(), DocumentError> {
        let path = generated_path(&self.kind)?;
        self.original_kind = Some(Box::new(self.kind.clone()));
        self.kind = path;
        Ok(())
    }

    /// Restore only an unmodified generated path, including its bounds/fill rule.
    pub fn convert_back(&mut self) -> Result<(), DocumentError> {
        let original = self
            .original_kind
            .as_ref()
            .ok_or_else(|| DocumentError("path has no original rect/circle geometry".into()))?;
        if self.kind != generated_path(original)? {
            return Err(DocumentError(
                "path geometry has been modified; cannot convert back".into(),
            ));
        }
        self.kind = *self.original_kind.take().unwrap();
        Ok(())
    }
}
