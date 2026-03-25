use super::{
    Renderer, iter_to_color,
    viewer::{self, Draggable, DragState, DragEvent, PanOrZoom},
};
use skia_safe::{AlphaType, Canvas, ColorType, Data, ImageInfo, Rect};
use winit::event::WindowEvent;

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

/// Interactive Mandelbrot set renderer
/// Supports panning with arrow keys and zooming with +/-
///
/// Calculation: iterate `z = z^2 + c`, where `z_0` is fixed and `c` varies per pixel.
/// This is the same calculation as the Julia set, but Julia fixes `c` and varies `z_0`
///
/// Computation runs on a background thread so the main loop stays responsive
/// Uses progressive refinement: renders the full image at coarse resolution
/// first, then refines in multiple passes (64 -> 32 -> 16 -> ... -> 1)
pub struct MandelbrotRenderer {
    center_re: f64,
    center_im: f64,
    /// Half-width of the viewport in the complex plane
    scale: f64,
    width: u32,
    height: u32,
    dirty: bool,
    computing: bool,
    buffer: Arc<RwLock<SharedBuffer>>,
    /// Set to true to signal the compute thread to stop early
    cancel: Arc<AtomicBool>,

    drag_state: DragState,
    cursor_pos: (f64, f64),
}

impl MandelbrotRenderer {
    pub fn new() -> Self {
        Self {
            center_re: -0.5, // slightly to left to show full view
            center_im: 0.0,
            scale: 1.5,
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
        }
    }

    /// Kick off a background thread that computes the Mandelbrot set
    /// using progressive refinement passes
    fn start_compute(&mut self) {
        // Create a new flag for the new thread
        self.cancel = Arc::new(AtomicBool::new(false));

        self.computing = true;
        self.dirty = false;

        let w = self.width;
        let h = self.height;
        let center_re = self.center_re;
        let center_im = self.center_im;
        let scale = self.scale;
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
                        // at the previous (coarser) block size
                        if block_size < INITIAL_BLOCK_SIZE
                            && px % (bs * 2) == 0
                            && py % (bs * 2) == 0
                        {
                            continue;
                        }

                        // Map pixel to complex plane coordinates
                        let c_re = center_re + (px as f64 / wu as f64 - 0.5) * 2.0 * half_w;
                        let c_im = center_im - (py as f64 / hu as f64 - 0.5) * 2.0 * half_h;
                        let iter = escape_time(c_re, c_im);
                        let color = iter_to_color(iter, MAX_ITER);

                        // Fill the block with this color
                        let mut buf = buffer.write().unwrap();
                        let bw = bs.min(wu - px);
                        let bh = bs.min(hu - py);
                        for by in 0..bh {
                            // for bx in 0..bw {
                            //     buf.pixels[(py + by) * wu + (px + bx)] = color;
                            // }
                            let offset = (py + by) * wu + px;
                            buf.pixels[offset..(offset+bw)].fill(color);
                        }
                    }
                }

                // Update the current block size so the render thread knows
                // the resolution of the latest completed pass
                {
                    let mut buf = buffer.write().unwrap();
                    buf.current_block_size = block_size;
                }

                block_size /= 2;
            }

            // Mark computation as complete
            buffer.write().unwrap().done = true;
        });
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

        // Start a background compute if needed
        if self.dirty && w > 0 && h > 0 {
            // Cancel any in-flight computation
            self.cancel.store(true, Ordering::Relaxed);
            self.start_compute();
        }

        // Read the shared buffer and draw whatever's been computed so far
        let buf = self.buffer.read().unwrap();
        if buf.width > 0 && buf.height > 0 {
            /*
            use skia_safe::{Color, Paint, Rect};
            let mut paint = Paint::default();
            // Draw blocks as rects of N*N pixels
            // TODO: use skia Image::from_raster for better performance
            let block_size = buf.current_block_size as usize;
            for py in (0..buf.height).step_by(block_size) {
                for px in (0..buf.width).step_by(block_size) {
                    let argb = buf.pixels[(py * buf.width + px) as usize];
                    paint.set_color(Color::from(argb));
                    canvas.draw_rect(
                        Rect::from_xywh(
                            bounds.left + px as f32,
                            bounds.top + py as f32,
                            block_size as f32,
                            block_size as f32,
                        ),
                        &paint,
                    );
                }
            }
            */

            // Blit the pixel buffer as a raster image in one draw call
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

        // Check if computation finished
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
        let drag_event = self.handle_drag_event(event, self.width, self.height, self.scale);
        let action = match drag_event {
            DragEvent::None => PanOrZoom::None,
            DragEvent::Move(dx, dy) => PanOrZoom::Pan(dx, dy),
            DragEvent::Drag(_, dx, dy) => PanOrZoom::Pan(dx, dy),
            DragEvent::Zoom(factor) => PanOrZoom::Zoom(factor),
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

impl Draggable for MandelbrotRenderer {
    fn set_cursor_pos(&mut self, x: f64, y: f64) { self.cursor_pos = (x, y); }
    fn get_cursor_pos(&mut self) -> (f64, f64) { self.cursor_pos }
    fn set_drag_state(&mut self, drag_state: DragState) { self.drag_state = drag_state; }
    fn get_drag_state(&self) -> &DragState { &self.drag_state }
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
