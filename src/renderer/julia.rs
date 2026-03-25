use super::{
    Renderer, iter_to_color,
    viewer::{self, Draggable, DragState, PanOrZoom},
};
use skia_safe::{AlphaType, Canvas, ColorType, Data, ImageInfo, Rect};
use winit::event::{ElementState, WindowEvent, KeyEvent};
use winit::keyboard::Key;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;

/// Maximum iterations for the escape-time algorithm
const MAX_ITER: u32 = 256;

/// Starting block size for progressive refinement (powers of 2)
const INITIAL_BLOCK_SIZE: u32 = 64;

/// Shared pixel buffer (compute thread writes, render thread reads)
struct SharedBuffer {
    pixels: Vec<u32>,
    width: u32,
    height: u32,
    current_block_size: u32,
    done: bool,
}

/// Interactive Julia set renderer
/// Supports panning with arrow keys and zooming with +/-
///
/// Calculation: iterate `z = z^2 + c`, where `c` is fixed and `z_0` varies per pixel.
/// This is the same calculation as the Mandelbrot set, but Mandelbrot varies `c` and fixes `z_0` 
pub struct JuliaRenderer {
    /// The fixed complex constant c = c_re + c_im*i
    c_re: f64,
    c_im: f64,
    /// Center of the viewport in the z-plane
    center_re: f64,
    center_im: f64,
    /// Half-width of the viewport
    scale: f64,
    width: u32,
    height: u32,
    dirty: bool,
    computing: bool,
    buffer: Arc<RwLock<SharedBuffer>>,
    cancel: Arc<AtomicBool>,
    drag_state: DragState,
    cursor_pos: (f64, f64),
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

impl JuliaRenderer {
    pub fn new(c_re: f64, c_im: f64) -> Self {
        Self {
            c_re,
            c_im,
            center_re: 0.0,
            center_im: 0.0,
            scale: 1.8,
            width: 0,
            height: 0,
            dirty: true,
            computing: false,
            buffer: Arc::new(RwLock::new(SharedBuffer {
                pixels: Vec::new(),
                width: 0,
                height: 0,
                current_block_size: INITIAL_BLOCK_SIZE,
                done: true,
            })),
            cancel: Arc::new(AtomicBool::new(false)),
            drag_state: DragState::None,
            cursor_pos: (0.0, 0.0),
            preset_index: 0,
        }
    }

    /// Kick off a background thread that computes the Julia set
    /// using progressive refinement passes
    fn start_compute(&mut self) {
        self.cancel = Arc::new(AtomicBool::new(false));
        self.computing = true;
        self.dirty = false;

        let w = self.width;
        let h = self.height;
        let center_re = self.center_re;
        let center_im = self.center_im;
        let scale = self.scale;
        let c_re = self.c_re;
        let c_im = self.c_im;
        let buffer = Arc::clone(&self.buffer);
        let cancel = Arc::clone(&self.cancel);

        // Initialize the shared buffer for the new computation.
        {
            let mut buf = buffer.write().unwrap();
            buf.pixels.resize((w * h) as usize, 0);
            buf.width = w;
            buf.height = h;
            buf.current_block_size = INITIAL_BLOCK_SIZE;
            buf.done = false;
        }

        thread::spawn(move || {
            let wu = w as usize;
            let hu = h as usize;
            let aspect = wu as f64 / hu.max(1) as f64;
            let half_w = scale * aspect;
            let half_h = scale;

            let mut block_size = INITIAL_BLOCK_SIZE;

            while block_size >= 1 {
                let bs = block_size as usize;

                for py in (0..hu).step_by(bs) {
                    // Check for cancellation at each row
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }

                    for px in (0..wu).step_by(bs) {
                        // On refinement passes, skip pixels already computed
                        if block_size < INITIAL_BLOCK_SIZE
                            && px % (bs * 2) == 0
                            && py % (bs * 2) == 0
                        {
                            continue;
                        }

                        // Map pixel to z-plane coordinates (this is z₀ for Julia)
                        let z_re = center_re + (px as f64 / wu as f64 - 0.5) * 2.0 * half_w;
                        let z_im = center_im - (py as f64 / hu as f64 - 0.5) * 2.0 * half_h;
                        let iter = julia_escape_time(z_re, z_im, c_re, c_im);
                        let color = iter_to_color(iter, MAX_ITER);

                        // Fill the block with this color
                        let mut buf = buffer.write().unwrap();
                        let bw = bs.min(wu - px);
                        let bh = bs.min(hu - py);
                        for by in 0..bh {
                            let offset = (py + by) * wu + px;
                            buf.pixels[offset..(offset + bw)].fill(color);
                        }
                    }
                }

                {
                    let mut buf = buffer.write().unwrap();
                    buf.current_block_size = block_size;
                }

                block_size /= 2;
            }

            buffer.write().unwrap().done = true;
        });
    }
}

impl Renderer for JuliaRenderer {
    fn render(&mut self, canvas: &Canvas, bounds: Rect) {
        let w = bounds.width() as u32;
        let h = bounds.height() as u32;

        if w != self.width || h != self.height {
            self.width = w;
            self.height = h;
            self.dirty = true;
        }

        if self.dirty && w > 0 && h > 0 {
            self.cancel.store(true, Ordering::Relaxed);
            self.start_compute();
        }

        let buf = self.buffer.read().unwrap();
        if buf.width > 0 && buf.height > 0 {
            let info = ImageInfo::new(
                (w as i32, h as i32),
                ColorType::BGRA8888,
                AlphaType::Premul,
                None,
            );
            let row_bytes = w as usize * 4;
            let pixel_bytes: &[u8] = bytemuck::cast_slice(&buf.pixels);
            let data = Data::new_copy(pixel_bytes);
            if let Some(image) = skia_safe::images::raster_from_data(&info, data, row_bytes) {
                canvas.draw_image(&image, (bounds.left, bounds.top), None);
            }
        }

        if buf.done && self.computing {
            drop(buf);
            self.computing = false;
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
        let action = match event {
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                if key_event.state != ElementState::Pressed {
                    return;
                }
                match &key_event.logical_key {
                    // Press 'c' to cycle through preset Julia constants
                    Key::Character(ch) if ch.as_str() == "c" => {
                        self.preset_index = (self.preset_index + 1) % PRESETS.len();
                        let (re, im) = PRESETS[self.preset_index];
                        self.c_re = re;
                        self.c_im = im;
                        self.dirty = true;
                    }
                    _ => {}
                }

                self.handle_drag_event(event, self.width, self.height, self.scale)
            }
            WindowEvent::MouseInput { .. } |
            WindowEvent::CursorMoved { .. } |
            WindowEvent::MouseWheel { .. } => {
                self.handle_drag_event(event, self.width, self.height, self.scale)
            }
            _ => PanOrZoom::None
        };

        match action {
            PanOrZoom::Pan(dx, dy) => {
                self.center_re += dx;
                self.center_im += dy;
                self.dirty = true;
            },
            PanOrZoom::Zoom(factor) => {
                self.scale *= factor;
                self.dirty = true;
            },
            PanOrZoom::None => {},
        }
    }
}

impl Draggable for JuliaRenderer {
    fn set_cursor_pos(&mut self, x: f64, y: f64) { self.cursor_pos = (x, y); }
    fn get_cursor_pos(&mut self) -> (f64, f64) { self.cursor_pos }
    fn set_drag_state(&mut self, drag_state: DragState) { self.drag_state = drag_state; }
    fn get_drag_state(&self) -> &DragState { &self.drag_state }
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
