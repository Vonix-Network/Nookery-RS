//! Touch-control placement and geometry from `pc/src/ui/touch_layout.cpp`.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// Minimum user-configurable touch-control scale.
pub const MIN_SCALE: f32 = 0.5;
/// Maximum user-configurable touch-control scale.
pub const MAX_SCALE: f32 = 2.5;

/// Anchor used to interpret a placement's x/y offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TouchAnchor {
    /// Top-left corner.
    TopLeft,
    /// Top edge center.
    Top,
    /// Top-right corner.
    TopRight,
    /// Bottom-left corner.
    BottomLeft,
    /// Bottom edge center.
    Bottom,
    /// Bottom-right corner.
    BottomRight,
}

impl TouchAnchor {
    /// JSON spelling used by the C++ implementation.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::TopLeft => "topLeft",
            Self::Top => "top",
            Self::TopRight => "topRight",
            Self::BottomLeft => "bottomLeft",
            Self::Bottom => "bottom",
            Self::BottomRight => "bottomRight",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "topLeft" => Self::TopLeft,
            "top" => Self::Top,
            "topRight" => Self::TopRight,
            "bottomLeft" => Self::BottomLeft,
            "bottom" => Self::Bottom,
            "bottomRight" => Self::BottomRight,
            _ => return None,
        })
    }
}

/// A stored touch-control placement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchPlacement {
    /// Horizontal offset from the selected anchor.
    pub x: f32,
    /// Vertical offset from the selected anchor.
    pub y: f32,
    /// Visual scale.
    pub scale: f32,
    /// Position anchor.
    pub anchor: TouchAnchor,
}

impl Default for TouchPlacement {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            anchor: TouchAnchor::TopLeft,
        }
    }
}

/// The persisted touch layout.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TouchLayout {
    /// Placements keyed by the stable control id.
    pub controls: BTreeMap<String, TouchPlacement>,
}

/// A resolved visual rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchRect {
    /// Left coordinate.
    pub l: f32,
    /// Top coordinate.
    pub t: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

/// Available area for touch controls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchSize {
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

/// Static metadata and default placement for one touch control.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchControlInfo {
    /// Stable persisted id.
    pub id: &'static str,
    /// UI element id.
    pub element_id: &'static str,
    /// Unscaled width.
    pub w: f32,
    /// Unscaled height.
    pub h: f32,
    /// Default placement.
    pub defaults: TouchPlacement,
}

const CONTROLS: [TouchControlInfo; 11] = [
    info(
        "triggerL",
        "trigger-l",
        78.0,
        46.0,
        24.0,
        18.0,
        TouchAnchor::TopLeft,
    ),
    info(
        "triggerR",
        "trigger-r",
        78.0,
        46.0,
        24.0,
        18.0,
        TouchAnchor::TopRight,
    ),
    info(
        "buttonZ",
        "button-z",
        78.0,
        46.0,
        24.0,
        72.0,
        TouchAnchor::TopRight,
    ),
    info(
        "buttonStart",
        "button-start",
        78.0,
        46.0,
        0.0,
        18.0,
        TouchAnchor::Top,
    ),
    info(
        "buttonMenu",
        "button-menu",
        46.0,
        46.0,
        70.0,
        18.0,
        TouchAnchor::Top,
    ),
    info(
        "dpad",
        "dpad",
        102.0,
        102.0,
        14.0,
        74.0,
        TouchAnchor::TopLeft,
    ),
    info(
        "cStick",
        "c-stick",
        58.0,
        58.0,
        8.0,
        6.0,
        TouchAnchor::BottomRight,
    ),
    info(
        "buttonA",
        "button-a",
        74.0,
        74.0,
        62.0,
        64.0,
        TouchAnchor::BottomRight,
    ),
    info(
        "buttonB",
        "button-b",
        58.0,
        58.0,
        158.0,
        48.0,
        TouchAnchor::BottomRight,
    ),
    info(
        "buttonX",
        "button-x",
        58.0,
        58.0,
        28.0,
        144.0,
        TouchAnchor::BottomRight,
    ),
    info(
        "buttonY",
        "button-y",
        58.0,
        58.0,
        124.0,
        138.0,
        TouchAnchor::BottomRight,
    ),
];

const fn info(
    id: &'static str,
    element_id: &'static str,
    w: f32,
    h: f32,
    x: f32,
    y: f32,
    anchor: TouchAnchor,
) -> TouchControlInfo {
    TouchControlInfo {
        id,
        element_id,
        w,
        h,
        defaults: TouchPlacement {
            x,
            y,
            scale: 1.0,
            anchor,
        },
    }
}

/// Return the source-order static control table.
#[must_use]
pub const fn controls() -> &'static [TouchControlInfo; 11] {
    &CONTROLS
}

/// Serialize the layout to the versioned JSON object used by the PC port.
#[must_use]
pub fn to_json(layout: &TouchLayout) -> Value {
    let mut controls = Map::new();
    for (id, placement) in &layout.controls {
        let mut entry = Map::new();
        entry.insert("x".to_owned(), Value::from(f64::from(placement.x)));
        entry.insert("y".to_owned(), Value::from(f64::from(placement.y)));
        entry.insert("scale".to_owned(), Value::from(f64::from(placement.scale)));
        entry.insert(
            "anchor".to_owned(),
            Value::String(placement.anchor.name().to_owned()),
        );
        controls.insert(id.clone(), Value::Object(entry));
    }
    let mut root = Map::new();
    root.insert("version".to_owned(), Value::from(1));
    root.insert("controls".to_owned(), Value::Object(controls));
    Value::Object(root)
}

/// Parse the layout JSON, clearing the destination before reading it.
///
/// Malformed or unknown control entries are skipped exactly as in the C++
/// `from_json` loop. Missing `controls`, or a non-object value, produces the
/// empty layout.
pub fn from_json(json: &Value) -> TouchLayout {
    let mut layout = TouchLayout::default();
    let Some(controls) = json.get("controls").and_then(Value::as_object) else {
        return layout;
    };
    for (id, entry) in controls {
        let Some(entry) = entry.as_object() else {
            continue;
        };
        let Some(anchor_name) = entry.get("anchor").and_then(Value::as_str) else {
            continue;
        };
        let Some(anchor) = TouchAnchor::from_name(anchor_name) else {
            continue;
        };
        layout.controls.insert(
            id.clone(),
            TouchPlacement {
                x: number(entry.get("x"), 0.0),
                y: number(entry.get("y"), 0.0),
                scale: number(entry.get("scale"), 1.0).clamp(MIN_SCALE, MAX_SCALE),
                anchor,
            },
        );
    }
    layout
}

fn number(value: Option<&Value>, default: f32) -> f32 {
    value.and_then(Value::as_f64).map_or(default, |v| v as f32)
}

/// Find a stored placement or return the static default.
#[must_use]
pub fn placement_for(layout: &TouchLayout, info: &TouchControlInfo) -> TouchPlacement {
    layout
        .controls
        .get(info.id)
        .copied()
        .unwrap_or(info.defaults)
}

fn anchor_left(anchor: TouchAnchor) -> bool {
    matches!(anchor, TouchAnchor::TopLeft | TouchAnchor::BottomLeft)
}

fn anchor_right(anchor: TouchAnchor) -> bool {
    matches!(anchor, TouchAnchor::TopRight | TouchAnchor::BottomRight)
}

fn anchor_bottom(anchor: TouchAnchor) -> bool {
    matches!(
        anchor,
        TouchAnchor::BottomLeft | TouchAnchor::Bottom | TouchAnchor::BottomRight
    )
}

fn round_to(value: f32, step: f32) -> f32 {
    (value / step).round() * step
}

/// Resolve a placement to the on-screen rectangle used by the touch renderer.
#[must_use]
pub fn resolve_visual(
    info: &TouchControlInfo,
    placement: TouchPlacement,
    area: TouchSize,
) -> TouchRect {
    let mut rect = TouchRect {
        l: 0.0,
        t: 0.0,
        w: info.w * placement.scale,
        h: info.h * placement.scale,
    };
    rect.l = if anchor_left(placement.anchor) {
        placement.x
    } else if anchor_right(placement.anchor) {
        area.w - placement.x - rect.w
    } else {
        area.w * 0.5 + placement.x - rect.w * 0.5
    };
    rect.t = if anchor_bottom(placement.anchor) {
        area.h - placement.y - rect.h
    } else {
        placement.y
    };
    rect
}

/// Encode a visual rectangle into the same rounded anchor representation as
/// the touch-controls editor.
#[must_use]
pub fn encode_placement(visual: TouchRect, scale: f32, area: TouchSize) -> TouchPlacement {
    let center_x = visual.l + visual.w * 0.5;
    let bottom = visual.t + visual.h * 0.5 >= area.h * 0.5;
    let mut placement = TouchPlacement {
        x: 0.0,
        y: 0.0,
        scale: round_to(scale, 0.01),
        anchor: TouchAnchor::TopLeft,
    };
    if center_x < area.w / 3.0 {
        placement.anchor = if bottom {
            TouchAnchor::BottomLeft
        } else {
            TouchAnchor::TopLeft
        };
        placement.x = visual.l;
    } else if center_x > area.w * 2.0 / 3.0 {
        placement.anchor = if bottom {
            TouchAnchor::BottomRight
        } else {
            TouchAnchor::TopRight
        };
        placement.x = area.w - visual.l - visual.w;
    } else {
        placement.anchor = if bottom {
            TouchAnchor::Bottom
        } else {
            TouchAnchor::Top
        };
        placement.x = center_x - area.w * 0.5;
    }
    placement.y = if bottom {
        area.h - visual.t - visual.h
    } else {
        visual.t
    };
    placement.x = round_to(placement.x, 0.1);
    placement.y = round_to(placement.y, 0.1);
    placement
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_source_order_and_geometry() {
        assert_eq!(controls()[0].id, "triggerL");
        let rect = resolve_visual(
            &controls()[7],
            controls()[7].defaults,
            TouchSize { w: 640.0, h: 480.0 },
        );
        assert_eq!(
            rect,
            TouchRect {
                l: 504.0,
                t: 342.0,
                w: 74.0,
                h: 74.0
            }
        );
    }

    #[test]
    fn json_round_trip_clamps_scale_and_skips_unknown_anchors() {
        let json = serde_json::json!({
            "version": 1,
            "controls": {
                "buttonA": { "x": 1.5, "y": 2.5, "scale": 9.0, "anchor": "bottomRight" },
                "ignored": { "anchor": "unknown" },
                "notObject": 3
            }
        });
        let layout = from_json(&json);
        assert_eq!(layout.controls.len(), 1);
        assert_eq!(layout.controls["buttonA"].scale, MAX_SCALE);
        assert_eq!(from_json(&to_json(&layout)), layout);
    }

    #[test]
    fn encoder_uses_same_one_decimal_rounding_and_anchor_thresholds() {
        let placement = encode_placement(
            TouchRect {
                l: 10.04,
                t: 20.06,
                w: 50.0,
                h: 50.0,
            },
            1.234,
            TouchSize { w: 640.0, h: 480.0 },
        );
        assert_eq!(placement.anchor, TouchAnchor::TopLeft);
        assert_eq!(placement.x, 10.0);
        assert_eq!(placement.y, 20.1);
        assert_eq!(placement.scale, 1.23);
    }
}
