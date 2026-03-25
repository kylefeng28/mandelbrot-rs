use skia_safe::{AlphaType, Canvas, ColorType, Data, ImageInfo, Rect};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;

/// Starting block size for progressive refinement (powers of 2).
/// First pass samples every 64th pixel, then 32, 16, 8, 4, 2, 1.
const INITIAL_BLOCK_SIZE: u32 = 64;

/// Shared pixel buffer: (compute thread writes rows progressively, render thread reads)
struct SharedBuffer {
    pixels: Vec<u32>,
    width: u32,
    height: u32,
    current_block_size: u32,
    done: bool,
}

/// Trait for the per-pixel computation of a fractal.
/// Implementors provide the function that maps a complex coordinate to a color.
///
/// Must be Send + Sync so it can be cloned/sent to the compute thread.
pub trait FractalCompute: Send + Sync {
    /// Compute the color (BGRA8888 u32) for the given point in the complex plane.
    fn compute_pixel(&self, x: f64, y: f64) -> u32;
}

/// Handles progressive refinement rendering for any fractal
/// Owns the shared buffer, cancel flag, and compute thread lifecycle
pub struct ProgressiveRenderer {
    pub width: u32,
    pub height: u32,
    pub dirty: bool,
    computing: bool,
    buffer: Arc<RwLock<SharedBuffer>>,
    cancel: Arc<AtomicBool>,
}

impl ProgressiveRenderer {
    pub fn new() -> Self {
        Self {
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
        }
    }

    /// Mark as dirty so the next render() triggers a recompute.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Update dimensions. Returns true if they changed.
    pub fn set_size(&mut self, width: u32, height: u32) -> bool {
        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Start a background compute using the given fractal computation and viewport.
    /// `center` and `scale` define the viewport in the complex plane.
    fn start_compute(
        &mut self,
        compute: Arc<dyn FractalCompute>,
        center_re: f64,
        center_im: f64,
        scale: f64,
    ) {
        // Cancel any in-flight computation
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));

        self.computing = true;
        self.dirty = false;

        let w = self.width;
        let h = self.height;
        let buffer = Arc::clone(&self.buffer);
        let cancel = Arc::clone(&self.cancel);

        // Initialize the shared buffer
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
                        let x = center_re + (px as f64 / wu as f64 - 0.5) * 2.0 * half_w;
                        let y = center_im - (py as f64 / hu as f64 - 0.5) * 2.0 * half_h;
                        let color = compute.compute_pixel(x, y);

                        // Fill the block with this color
                        let mut buf = buffer.write().unwrap();
                        let bw = bs.min(wu - px);
                        let bh = bs.min(hu - py);
                        for by in 0..bh {
                            // for bx in 0..bw {
                            //     buf.pixels[(py + by) * wu + (px + bx)] = color;
                            // }
                            let offset = (py + by) * wu + px;
                            buf.pixels[offset..(offset + bw)].fill(color);
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

            buffer.write().unwrap().done = true;
        });
    }

    /// Call from your Renderer::render(). Kicks off compute if dirty,
    /// blits the current buffer to the canvas.
    pub fn render(
        &mut self,
        canvas: &Canvas,
        bounds: Rect,
        compute: Arc<dyn FractalCompute>,
        center_re: f64,
        center_im: f64,
        scale: f64,
    ) {
        let w = bounds.width() as u32;
        let h = bounds.height() as u32;

        self.set_size(w, h);

        if self.dirty && w > 0 && h > 0 {
            self.start_compute(compute, center_re, center_im, scale);
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
                (buf.width as i32, buf.height as i32),
                ColorType::BGRA8888,
                AlphaType::Premul,
                None,
            );
            let row_bytes = buf.width as usize * 4;
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
}
