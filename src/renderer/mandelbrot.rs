use super::Renderer;
use skia_safe::{Canvas, Color, Paint, Rect};
use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{Key, NamedKey};

/// Maximum iterations for the escape-time algorithm
const MAX_ITER: u32 = 256;

/// Interactive Mandelbrot set renderer
/// Supports panning with arrow keys and zooming with +/-
pub struct MandelbrotRenderer {
    center_re: f64,
    center_im: f64,
    /// Half-width of the viewport in the complex plane
    scale: f64,
    /// Cached pixel buffer (ARGB). Regenerated on parameter change
    pixels: Vec<u32>,
    width: u32,
    height: u32,
    dirty: bool,
}

impl MandelbrotRenderer {
    pub fn new() -> Self {
        Self {
            center_re: -0.5, // slightly to left to show full view
            center_im: 0.0,
            scale: 1.5,
            pixels: Vec::new(),
            width: 0,
            height: 0,
            dirty: true,
        }
    }

    fn recompute(&mut self) {
        let w = self.width as usize;
        let h = self.height as usize;
        self.pixels.resize(w * h, 0);

        let aspect = w as f64 / h.max(1) as f64;
        let half_w = self.scale * aspect;
        let half_h = self.scale;

        for py in 0..h {
            for px in 0..w {
                // Map pixel to complex plane coordinates
                let c_re = self.center_re + (px as f64 / w as f64 - 0.5) * 2.0 * half_w;
                let c_im = self.center_im - (py as f64 / h as f64 - 0.5) * 2.0 * half_h;

                let iter = escape_time(c_re, c_im);
                self.pixels[py * w + px] = iter_to_color(iter);
            }
        }

        self.dirty = false;
    }
}

impl Renderer for MandelbrotRenderer {
    fn render(&mut self, canvas: &Canvas, bounds: Rect) {
        let w = bounds.width() as u32;
        let h = bounds.height() as u32;

        // Resize pixel buffer if bounds changed
        if w != self.width || h != self.height {
            self.width = w;
            self.height = h;
            self.dirty = true;
        }

        if self.dirty && w > 0 && h > 0 {
            self.recompute();
        }

        // Draw pixels as 1x1 rects. Not fast, but simple and correct
        // TODO: use skia Image::from_raster for better performance
        let mut paint = Paint::default();
        for py in 0..h {
            for px in 0..w {
                let argb = self.pixels[(py * w + px) as usize];
                paint.set_color(Color::from(argb));
                canvas.draw_rect(
                    Rect::from_xywh(bounds.left + px as f32, bounds.top + py as f32, 1.0, 1.0),
                    &paint,
                );
            }
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
            self.dirty = true;
        }
    }

    fn handle_event(&mut self, event: &WindowEvent) {
        if let WindowEvent::KeyboardInput { event, .. } = event {
            if event.state != ElementState::Pressed {
                return;
            }
            let pan_amount = self.scale * 0.1;
            match &event.logical_key {
                // Pan with arrow keys
                Key::Named(NamedKey::ArrowLeft) =>  { self.center_re -= pan_amount; self.dirty = true; }
                Key::Named(NamedKey::ArrowRight) => { self.center_re += pan_amount; self.dirty = true; }
                Key::Named(NamedKey::ArrowUp) =>    { self.center_im += pan_amount; self.dirty = true; }
                Key::Named(NamedKey::ArrowDown) =>  { self.center_im -= pan_amount; self.dirty = true; }

                // Zoom with +/-
                Key::Character(ch) if ch.as_str() == "=" || ch.as_str() == "+" => {
                    self.scale *= 0.8;
                    self.dirty = true;
                }
                Key::Character(ch) if ch.as_str() == "-" => {
                    self.scale *= 1.25;
                    self.dirty = true;
                }
                _ => {}
            }
        }
    }
}

/// Escape-time algorithm: returns iteration count (0..MAX_ITER)
/// Returns MAX_ITER if the point is in the set
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

/// Map iteration count to a color (ARGB u32)
/// Points in the set are black; escaped points get a smooth gradient
fn iter_to_color(iter: u32) -> u32 {
    if iter == MAX_ITER {
        return 0xff_000000; // Black for points in the set
    }

    // Smooth coloring using a simple palette
    let t = iter as f64 / MAX_ITER as f64;
    let r = (9.0 * (1.0 - t) * t * t * t * 255.0) as u32;
    let g = (15.0 * (1.0 - t) * (1.0 - t) * t * t * 255.0) as u32;
    let b = (8.5 * (1.0 - t) * (1.0 - t) * (1.0 - t) * t * 255.0) as u32;
    0xff_000000 | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255)
}
