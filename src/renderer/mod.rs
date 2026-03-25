#![allow(unused)]
pub mod viewer;

pub mod icon_renderer;
pub mod mandelbrot;

use skia_safe::{Canvas, Rect};
use winit::event::WindowEvent;

#[allow(unused)]
pub trait Renderer {
    /// Render content into the given region of the canvas.
    fn render(&mut self, canvas: &Canvas, bounds: Rect);

    /// Called when the content area is resized.
    fn resize(&mut self, _width: u32, _height: u32) {}

    /// Handler for window events (e.g. key press)
    fn handle_event(&mut self, event: &WindowEvent) {}
}
