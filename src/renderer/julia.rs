use super::{
    Renderer, iter_to_color,
    escape_time::EscapeTimeRenderer,
    progressive::FractalCompute,
    viewer::{Draggable, DragState, DragEvent, PanOrZoom},
};
use skia_safe::{Canvas, Rect};
use winit::event::{ElementState, WindowEvent, MouseButton};
use winit::keyboard::Key;
use std::sync::Arc;

/// Maximum iterations for the escape-time algorithm
const MAX_ITER: u32 = 256;

/// Interactive Julia set renderer
/// Supports panning with arrow keys and zooming with +/-
///
/// Press 'c' to cycle through preset constants.
/// Right-click drag to change `c` interactively.
///
/// Calculation: iterate `z = z^2 + c`, where `c` is fixed and `z_0` varies per pixel.
/// This is the same calculation as the Mandelbrot set, but Mandelbrot varies `c` and fixes `z_0`
pub struct JuliaRenderer {
    renderer: EscapeTimeRenderer<JuliaCompute>,

    /// The fixed complex constant c = c_re + c_im*i
    c_re: f64,
    c_im: f64,

    /// Index into the preset list for cycling with 'c'
    preset_index: usize,
}

/// Some visually interesting Julia set constants
pub const PRESETS: [(f64, f64); 6] = [
    (-0.7269, 0.1889),   // Dendrite-like
    (-0.8, 0.156),       // Spiral arms
    (0.285, 0.01),       // Near parabolic
    (-0.4, 0.6),         // Rabbit-like
    (0.355, 0.355),      // Disconnected dust
    (-0.54, 0.54),       // Branching tendrils
];

fn log_c(c_re: f64, c_im: f64) {
    println!("Using (c_re, c_im) = ({}, {})", c_re, c_im);
}

impl JuliaRenderer {
    pub fn new(c_re: f64, c_im: f64) -> Self {
        log_c(c_re, c_im);

        let compute = Self::build_compute(c_re, c_im);

        Self {
            renderer: EscapeTimeRenderer::new(compute),
            c_re,
            c_im,
            preset_index: 0,
        }
    }

    fn set_c(&mut self, c_re: f64, c_im: f64) {
        self.c_re = c_re;
        self.c_im = c_im;

        self.renderer.compute = Self::build_compute(c_re, c_im);

        log_c(c_re, c_im);
        self.renderer.mark_dirty();
    }

    /// Build the compute object for the current c value
    fn build_compute(c_re: f64, c_im: f64) -> Arc<JuliaCompute> {
        Arc::new(JuliaCompute { c_re, c_im })
    }
}

impl Renderer for JuliaRenderer {
    fn render(&mut self, canvas: &Canvas, bounds: Rect) {
        self.renderer.render(canvas, bounds);
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
    }

    fn handle_event(&mut self, event: &WindowEvent) {
        if let WindowEvent::KeyboardInput { event: key_event, .. } = event {
            if key_event.state == ElementState::Pressed {
                // Press 'c' to cycle through preset Julia constants
                if let Key::Character(ch) = &key_event.logical_key {
                    if ch.as_str() == "c" {
                        self.preset_index = (self.preset_index + 1) % PRESETS.len();
                        let (re, im) = PRESETS[self.preset_index];
                        self.set_c(re, im);
                    }
                }
            }
        }

        let drag_event = self.renderer.handle_drag_event(
            event, self.renderer.progressive.width, self.renderer.progressive.height, self.renderer.scale,
        );

        // Right-click drag: change c value
        if let DragEvent::Drag(MouseButton::Right, dx, dy) = drag_event {
            self.set_c(self.c_re + dx, self.c_im + dy);
        }

        let action = match drag_event {
            DragEvent::Move(dx, dy) => PanOrZoom::Pan(dx, dy),
            DragEvent::Drag(MouseButton::Left, dx, dy) => PanOrZoom::Pan(dx, dy),
            DragEvent::Zoom(factor) => PanOrZoom::Zoom(factor),
            _ => PanOrZoom::None,
        };
        self.renderer.handle_drag_action(&action);
    }
}

struct JuliaCompute {
    c_re: f64,
    c_im: f64,
}

impl FractalCompute for JuliaCompute {
    fn compute_pixel(&self, z_re: f64, z_im: f64) -> u32 {
        let z = julia_escape_time(z_re, z_im, self.c_re, self.c_im);
        iter_to_color(z, MAX_ITER)
    }
}


/// Julia set escape-time: iterate z = z^2 + c starting from z_0 = (z_re, z_im)
/// with fixed c = (c_re, c_im)
fn julia_escape_time(mut z_re: f64, mut z_im: f64, c_re: f64, c_im: f64) -> u32 {
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
