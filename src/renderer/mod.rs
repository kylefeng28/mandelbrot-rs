#![allow(unused)]
pub mod viewer;

pub mod icon_renderer;
pub mod julia;
pub mod mandelbrot;
pub mod progressive;
pub mod escape_time;

use skia_safe::{Canvas, Rect};
use winit::event::WindowEvent;

pub trait Renderer {
    /// Render content into the given region of the canvas.
    fn render(&mut self, canvas: &Canvas, bounds: Rect);

    /// Called when the content area is resized.
    fn resize(&mut self, _width: u32, _height: u32) {}

    /// Handler for window events (e.g. key press)
    fn handle_event(&mut self, event: &WindowEvent) {}
}

/// Map iteration count to a color (BGRA8888 u32)
/// Points in the set are black; escaped points get a smooth gradient
pub fn iter_to_color(iter: u32, max_iter: u32) -> u32 {
    if iter == max_iter {
        return 0xff_000000;
    }

    // Smooth coloring using a simple palette
    let t = iter as f64 / max_iter as f64;
    let r = (9.0 * (1.0 - t) * t * t * t * 255.0) as u32;
    let g = (15.0 * (1.0 - t) * (1.0 - t) * t * t * 255.0) as u32;
    let b = (8.5 * (1.0 - t) * (1.0 - t) * (1.0 - t) * t * 255.0) as u32;
    // BGRA8888 byte order: B | G << 8 | R << 16 | A << 24
    0xff_000000 | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255)
}
