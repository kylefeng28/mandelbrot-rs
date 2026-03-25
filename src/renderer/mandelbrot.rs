use super::{
    Renderer, iter_to_color,
    escape_time::EscapeTimeRenderer,
    progressive::{FractalCompute, ProgressiveRenderer},
    viewer::{Draggable, DragState, DragEvent, PanOrZoom},
};
use skia_safe::{Canvas, Rect};
use winit::event::WindowEvent;
use std::sync::Arc;

/// Maximum iterations for the escape-time algorithm
const MAX_ITER: u32 = 256;

/// Interactive Mandelbrot set renderer
/// Supports panning with arrow keys and zooming with +/-
///
/// Calculation: iterate `z = z^2 + c`, where `c` is fixed and `z_0` varies per pixel.
/// This is the same calculation as the Julia set, but Julia fixes `c` and varies `z_0`
pub struct MandelbrotRenderer {
    renderer: EscapeTimeRenderer<MandelbrotCompute>,
}

impl MandelbrotRenderer {
    pub fn new() -> Self {
        let compute = Arc::new(MandelbrotCompute);
        Self { renderer: EscapeTimeRenderer::new(compute), }
    }
}

impl Renderer for MandelbrotRenderer {
    fn render(&mut self, canvas: &Canvas, bounds: Rect) {
        self.renderer.render(canvas, bounds);
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
    }

    fn handle_event(&mut self, event: &WindowEvent) {
        self.renderer.handle_event(event);
    }
}

struct MandelbrotCompute;

impl FractalCompute for MandelbrotCompute {
    fn compute_pixel(&self, c_re: f64, c_im: f64) -> u32 {
        iter_to_color(escape_time(c_re, c_im), MAX_ITER)
    }
}
/// Escape-time algorithm: returns iteration count (0..MAX_ITER)
/// Returns MAX_ITER if the point is in the set
/// Mandelbrot set escape-time: iterate z = z^2 + c starting from z_0 = (z_re, z_im)
/// with with varying c = (c_re, c_im)
fn escape_time(c_re: f64, c_im: f64) -> u32 {
    let mut z_re = 0.0;
    let mut z_im = 0.0;
    for i in 0..MAX_ITER {
        let z_re2 = z_re * z_re;
        let z_im2 = z_im * z_im;
        if z_re2 + z_im2 > 4.0 {
            return i;
        }
        z_im = 2.0 * z_re * z_im + c_im;
        z_re = z_re2 - z_im2 + c_re;
    }
    MAX_ITER
}
