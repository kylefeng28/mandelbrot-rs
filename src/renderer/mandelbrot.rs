use super::{
    Renderer, iter_to_color,
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
    /// Center of the viewport in the z-plane
    center_re: f64,
    center_im: f64,
    /// Half-width of the viewport in the complex plane
    scale: f64,
    progressive: ProgressiveRenderer,
    compute: Arc<MandelbrotCompute>,
    drag_state: DragState,
    cursor_pos: (f64, f64),
}

impl MandelbrotRenderer {
    pub fn new() -> Self {
        Self {
            center_re: -0.5, // slightly to left to show full view
            center_im: 0.0,
            scale: 1.5,
            progressive: ProgressiveRenderer::new(),
            compute: Arc::new(MandelbrotCompute),
            drag_state: DragState::None,
            cursor_pos: (0.0, 0.0),
        }
    }
}

impl Renderer for MandelbrotRenderer {
    fn render(&mut self, canvas: &Canvas, bounds: Rect) {
        self.progressive.render(
            canvas, bounds,
            Arc::clone(&self.compute) as Arc<dyn FractalCompute>,
            self.center_re, self.center_im, self.scale,
        );
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.progressive.set_size(width, height);
    }

    fn handle_event(&mut self, event: &WindowEvent) {
        let drag_event = self.handle_drag_event(
            event, self.progressive.width, self.progressive.height, self.scale,
        );
        let action = match drag_event {
            DragEvent::Move(dx, dy) => PanOrZoom::Pan(dx, dy),
            DragEvent::Drag(_, dx, dy) => PanOrZoom::Pan(dx, dy),
            DragEvent::Zoom(factor) => PanOrZoom::Zoom(factor),
            DragEvent::None => PanOrZoom::None,
        };
        match action {
            PanOrZoom::Pan(dx, dy) => {
                self.center_re += dx;
                self.center_im += dy;
                self.progressive.mark_dirty();
            }
            PanOrZoom::Zoom(factor) => {
                self.scale *= factor;
                self.progressive.mark_dirty();
            }
            PanOrZoom::None => {}
        }
    }
}

impl Draggable for MandelbrotRenderer {
    fn set_cursor_pos(&mut self, x: f64, y: f64) { self.cursor_pos = (x, y); }
    fn get_cursor_pos(&mut self) -> (f64, f64) { self.cursor_pos }
    fn set_drag_state(&mut self, drag_state: DragState) { self.drag_state = drag_state; }
    fn get_drag_state(&self) -> &DragState { &self.drag_state }
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
