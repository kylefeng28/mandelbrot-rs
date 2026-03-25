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

/// Interactive renderer for escape-time fractals like the Mandelbrot set, Julia set, etc
/// Supports panning with arrow keys and zooming with +/-
pub struct EscapeTimeRenderer<T: FractalCompute> {
    compute: Arc<T>,

    /// Center of the viewport in the z-plane
    center_re: f64,
    center_im: f64,
    /// Half-width of the viewport in the complex plane
    scale: f64,
    progressive: ProgressiveRenderer,
    drag_state: DragState,
    cursor_pos: (f64, f64),
}

impl<T: FractalCompute> EscapeTimeRenderer<T> {
    pub fn new(compute: Arc<T>) -> Self {
        Self {
            compute,

            center_re: -0.5, // slightly to left to show full view
            center_im: 0.0,
            scale: 1.5,
            progressive: ProgressiveRenderer::new(),
            drag_state: DragState::None,
            cursor_pos: (0.0, 0.0),
        }
    }
}

impl<T: FractalCompute + 'static> Renderer for EscapeTimeRenderer<T> {
    fn render(&mut self, canvas: &Canvas, bounds: Rect) {
        self.progressive.render(
            canvas, bounds,
            self.compute.clone(),
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

impl<T: FractalCompute> Draggable for EscapeTimeRenderer<T> {
    fn set_cursor_pos(&mut self, x: f64, y: f64) { self.cursor_pos = (x, y); }
    fn get_cursor_pos(&mut self) -> (f64, f64) { self.cursor_pos }
    fn set_drag_state(&mut self, drag_state: DragState) { self.drag_state = drag_state; }
    fn get_drag_state(&self) -> &DragState { &self.drag_state }
}
