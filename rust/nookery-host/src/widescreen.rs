//! Widescreen decisions from `pc/src/aurora_widescreen.cpp`.

/// GameCube render width.
pub const GC_WIDTH: f32 = 640.0;
/// GameCube render height.
pub const GC_HEIGHT: f32 = 480.0;

/// Calculate the hor+ correction factor used by the PC port.
#[must_use]
pub fn aspect_factor(window_width: i32, window_height: i32) -> f32 {
    if window_width <= 0 || window_height <= 0 {
        return 1.0;
    }
    let gc_aspect = GC_WIDTH / GC_HEIGHT;
    let render_aspect = window_width as f32 / window_height as f32;
    if render_aspect <= gc_aspect + 0.01 {
        1.0
    } else {
        gc_aspect / render_aspect
    }
}

/// Calculate the edge offset for HUD elements in edge-anchored mode.
#[must_use]
pub fn hud_edge_offset(hud_at_edges: bool, factor: f32) -> f32 {
    if !hud_at_edges {
        return 0.0;
    }
    GC_WIDTH * 0.25 * (1.0 / factor - 1.0)
}

/// Apply the C++ projection decision to a row-major 4×4 matrix.
///
/// Returns whether the matrix was changed. The GX upload remains outside this
/// crate. `orthographic` is the comparison with `GX_ORTHOGRAPHIC`.
pub fn adjust_projection(
    matrix: &mut [[f32; 4]; 4],
    mode: i32,
    orthographic: bool,
    factor: f32,
) -> bool {
    let correct = mode == 0 || (mode == 2 && orthographic);
    if correct && factor < 1.0 {
        matrix[0][0] *= factor;
        true
    } else {
        false
    }
}

/// A viewport after the mode-2 pillarbox adjustment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Left coordinate.
    pub left: f32,
    /// Top coordinate.
    pub top: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
    /// Near plane.
    pub nearz: f32,
    /// Far plane.
    pub farz: f32,
}

/// Apply the C++ viewport decision without calling GX.
#[must_use]
pub fn adjust_viewport(viewport: Viewport, mode: i32, factor: f32) -> Viewport {
    if mode != 2 || factor >= 1.0 {
        return viewport;
    }
    let full = viewport.left < 1.0
        && viewport.top < 1.0
        && viewport.width > GC_WIDTH - 1.0
        && viewport.height > GC_HEIGHT - 1.0;
    if full {
        return viewport;
    }
    let content_left = (1.0 - factor) * 0.5 * GC_WIDTH;
    Viewport {
        left: content_left + viewport.left * factor,
        top: viewport.top,
        width: viewport.width * factor,
        height: viewport.height,
        nearz: viewport.nearz,
        farz: viewport.farz,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspect_factor_matches_letterbox_boundary() {
        assert_eq!(aspect_factor(0, 480), 1.0);
        assert_eq!(aspect_factor(640, 480), 1.0);
        assert!(aspect_factor(1920, 1080) < 1.0);
    }

    #[test]
    fn mode_two_only_pillarboxes_non_full_viewports() {
        let factor = aspect_factor(1920, 1080);
        let full = Viewport {
            left: 0.0,
            top: 0.0,
            width: 640.0,
            height: 480.0,
            nearz: 0.0,
            farz: 1.0,
        };
        assert_eq!(adjust_viewport(full, 2, factor), full);
        let partial = Viewport {
            width: 100.0,
            height: 50.0,
            ..full
        };
        let adjusted = adjust_viewport(partial, 2, factor);
        assert!(adjusted.left > partial.left);
        assert!(adjusted.width < partial.width);
    }
}
