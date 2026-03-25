/// SmoothLife: a continuous generalization of Conway's Game of Life.
/// Stephan Rafler, 2011 — "Generalization of Conway's Game of Life to a continuous domain"
///
/// Instead of a discrete grid with {0,1} states, cells hold f32 values in [0,1].
/// Neighborhoods are concentric discs with smooth transition functions.
/// Uses FFT convolution for O(n² log n) per step instead of O(n² · ra²).
use super::Renderer;
use skia_safe::{
    AlphaType, Canvas, Color, ColorType, Data, ImageInfo, Rect,
};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key, NamedKey};
use rustfft::{FftPlanner, num_complex::Complex};

/// Grid dimensions for the simulation
const GRID_W: usize = 256;
const GRID_H: usize = 256;
const N: usize = GRID_W * GRID_H;

/// SmoothLife parameters
struct Params {
    /// Inner disc radius (cell state averaging)
    ri: f32,
    /// Outer ring radius (neighborhood)
    ra: f32,
    /// Birth interval [b1, b2]
    b1: f32,
    b2: f32,
    /// Death (survival) interval [d1, d2]
    d1: f32,
    d2: f32,
    /// Sigmoid width (sharpness of transitions)
    alpha_n: f32,
    alpha_m: f32,
    /// Time step
    dt: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            ri: 4.0,
            ra: 12.0,
            b1: 0.278,
            b2: 0.365,
            d1: 0.267,
            d2: 0.445,
            alpha_n: 0.028,
            alpha_m: 0.147,
            dt: 0.1,
        }
    }
}

pub struct SmoothLifeRenderer {
    grid: Vec<f32>,
    /// Precomputed pixel buffer (BGRA8888)
    pixels: Vec<u32>,
    params: Params,
    running: bool,
    /// Viewport
    offset_x: f64,
    offset_y: f64,
    cell_size: f64,
    width: u32,
    height: u32,
    cursor_pos: (f64, f64),
    drag: Option<(MouseButton, f64, f64)>,
    dragged: bool,
    /// Cached FFT kernels (recomputed when ri/ra change)
    inner_kernel_fft: Vec<Complex<f32>>,
    outer_kernel_fft: Vec<Complex<f32>>,
    cached_ri: f32,
    cached_ra: f32,
    fft_planner: FftPlanner<f32>,
}

/// Smooth sigmoid
fn sigma(x: f32, a: f32, alpha: f32) -> f32 {
    1.0 / (1.0 + (-(x - a) * 4.0 / alpha).exp())
}

/// Smooth interval function: ~1 when x in [a, b], ~0 outside
fn sigma_n(x: f32, a: f32, b: f32, alpha: f32) -> f32 {
    sigma(x, a, alpha) * (1.0 - sigma(x, b, alpha))
}

/// Smooth lerp between two intervals based on cell state m
fn sigma_m(x: f32, y: f32, m: f32, alpha: f32) -> f32 {
    x * (1.0 - sigma(m, 0.5, alpha)) + y * sigma(m, 0.5, alpha)
}

/// The SmoothLife transition function S(n, m)
fn transition(n: f32, m: f32, p: &Params) -> f32 {
    let alive_lo = sigma_m(p.b1, p.d1, m, p.alpha_m);
    let alive_hi = sigma_m(p.b2, p.d2, m, p.alpha_m);
    sigma_n(n, alive_lo, alive_hi, p.alpha_n)
}

/// Build a normalized disc kernel in frequency domain.
/// The kernel is centered at (0,0) with toroidal wrapping.
fn build_kernel_fft(radius: f32, planner: &mut FftPlanner<f32>) -> Vec<Complex<f32>> {
    let r_sq = radius * radius;
    let mut kernel = vec![Complex::new(0.0f32, 0.0); N];
    let mut count = 0.0f32;
    let ri = radius.ceil() as i32;

    for dy in -ri..=ri {
        for dx in -ri..=ri {
            if (dx * dx + dy * dy) as f32 <= r_sq {
                let x = (dx as isize).rem_euclid(GRID_W as isize) as usize;
                let y = (dy as isize).rem_euclid(GRID_H as isize) as usize;
                kernel[y * GRID_W + x].re += 1.0;
                count += 1.0;
            }
        }
    }

    // Normalize
    if count > 0.0 {
        for v in &mut kernel {
            v.re /= count;
        }
    }

    // 2D FFT via row-then-column
    fft_2d_forward(&mut kernel, planner);
    kernel
}

fn fft_2d_forward(data: &mut [Complex<f32>], planner: &mut FftPlanner<f32>) {
    let fft_w = planner.plan_fft_forward(GRID_W);
    let fft_h = planner.plan_fft_forward(GRID_H);

    // FFT each row
    for row in data.chunks_exact_mut(GRID_W) {
        fft_w.process(row);
    }

    // FFT each column (need to gather/scatter)
    let mut col_buf = vec![Complex::new(0.0f32, 0.0); GRID_H];
    for x in 0..GRID_W {
        for y in 0..GRID_H {
            col_buf[y] = data[y * GRID_W + x];
        }
        fft_h.process(&mut col_buf);
        for y in 0..GRID_H {
            data[y * GRID_W + x] = col_buf[y];
        }
    }
}

fn fft_2d_inverse(data: &mut [Complex<f32>], planner: &mut FftPlanner<f32>) {
    let ifft_w = planner.plan_fft_inverse(GRID_W);
    let ifft_h = planner.plan_fft_inverse(GRID_H);
    let scale = 1.0 / N as f32;

    // IFFT each row
    for row in data.chunks_exact_mut(GRID_W) {
        ifft_w.process(row);
    }

    // IFFT each column
    let mut col_buf = vec![Complex::new(0.0f32, 0.0); GRID_H];
    for x in 0..GRID_W {
        for y in 0..GRID_H {
            col_buf[y] = data[y * GRID_W + x];
        }
        ifft_h.process(&mut col_buf);
        for y in 0..GRID_H {
            data[y * GRID_W + x] = col_buf[y];
        }
    }

    // Normalize
    for v in data.iter_mut() {
        *v *= scale;
    }
}

/// Pointwise multiply in frequency domain
fn pointwise_mul(a: &[Complex<f32>], b: &[Complex<f32>], out: &mut [Complex<f32>]) {
    for i in 0..a.len() {
        out[i] = a[i] * b[i];
    }
}

impl SmoothLifeRenderer {
    pub fn new() -> Self {
        // Seed: random blob in center
        let mut grid = vec![0.0f32; N];
        let cx = GRID_W / 2;
        let cy = GRID_H / 2;
        let r = 20;
        for y in (cy - r)..(cy + r) {
            for x in (cx - r)..(cx + r) {
                let dx = x as f32 - cx as f32;
                let dy = y as f32 - cy as f32;
                if dx * dx + dy * dy < (r * r) as f32 {
                    // Use a simple deterministic pattern
                    let v = ((dx * 7.3 + dy * 13.7).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
                    grid[y * GRID_W + x] = v;
                }
            }
        }

        let params = Params::default();
        let mut planner = FftPlanner::new();

        // Build the outer ring kernel = disc(ra) - disc(ri), normalized
        let inner_kernel_fft = build_kernel_fft(params.ri, &mut planner);
        let outer_kernel_fft = build_ring_kernel_fft(params.ri, params.ra, &mut planner);

        Self {
            grid,
            pixels: vec![0; N],
            cached_ri: params.ri,
            cached_ra: params.ra,
            params,
            running: true,
            offset_x: 0.0,
            offset_y: 0.0,
            cell_size: 3.0,
            width: 0,
            height: 0,
            cursor_pos: (0.0, 0.0),
            drag: None,
            dragged: false,
            inner_kernel_fft,
            outer_kernel_fft,
            fft_planner: planner,
        }
    }

    fn rebuild_kernels(&mut self) {
        if self.params.ri != self.cached_ri || self.params.ra != self.cached_ra {
            self.inner_kernel_fft = build_kernel_fft(self.params.ri, &mut self.fft_planner);
            self.outer_kernel_fft = build_ring_kernel_fft(
                self.params.ri, self.params.ra, &mut self.fft_planner,
            );
            self.cached_ri = self.params.ri;
            self.cached_ra = self.params.ra;
        }
    }

    fn step(&mut self) {
        self.rebuild_kernels();

        // Forward FFT of grid
        let mut grid_fft: Vec<Complex<f32>> = self.grid.iter()
            .map(|&v| Complex::new(v, 0.0))
            .collect();
        fft_2d_forward(&mut grid_fft, &mut self.fft_planner);

        // Convolve with inner disc → m (cell state average)
        let mut m_fft = vec![Complex::new(0.0f32, 0.0); N];
        pointwise_mul(&grid_fft, &self.inner_kernel_fft, &mut m_fft);
        fft_2d_inverse(&mut m_fft, &mut self.fft_planner);

        // Convolve with outer ring → n (neighborhood average)
        let mut n_fft = vec![Complex::new(0.0f32, 0.0); N];
        pointwise_mul(&grid_fft, &self.outer_kernel_fft, &mut n_fft);
        fft_2d_inverse(&mut n_fft, &mut self.fft_planner);

        // Apply transition function pointwise
        let dt = self.params.dt;
        for i in 0..N {
            let m = m_fft[i].re;
            let n = n_fft[i].re;
            let s = transition(n, m, &self.params);
            self.grid[i] = (self.grid[i] + dt * (2.0 * s - 1.0)).clamp(0.0, 1.0);
        }
    }

    fn update_pixels(&mut self) {
        for (i, &v) in self.grid.iter().enumerate() {
            // Color map: black (0) → blue → cyan → white (1)
            let t = v.clamp(0.0, 1.0);
            let (r, g, b);
            if t < 0.5 {
                let s = t * 2.0;
                r = 0;
                g = (s * 180.0) as u32;
                b = (40.0 + s * 215.0) as u32;
            } else {
                let s = (t - 0.5) * 2.0;
                r = (s * 255.0) as u32;
                g = (180.0 + s * 75.0) as u32;
                b = 255;
            }
            // BGRA8888
            self.pixels[i] = 0xff_000000 | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255);
        }
    }

    fn randomize(&mut self) {
        let cx = GRID_W / 2;
        let cy = GRID_H / 2;
        let r = 30;
        self.grid.fill(0.0);
        // Deterministic "random" seed using sin
        let mut seed = 42u64;
        for y in (cy - r)..(cy + r) {
            for x in (cx - r)..(cx + r) {
                let dx = x as f32 - cx as f32;
                let dy = y as f32 - cy as f32;
                if dx * dx + dy * dy < (r * r) as f32 {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    self.grid[y * GRID_W + x] = (seed >> 33) as f32 / (1u64 << 31) as f32;
                }
            }
        }
    }

    fn pixel_to_grid(&self, px: f64, py: f64) -> (usize, usize) {
        let gx = ((px / self.cell_size + self.offset_x) as i32).rem_euclid(GRID_W as i32) as usize;
        let gy = ((py / self.cell_size + self.offset_y) as i32).rem_euclid(GRID_H as i32) as usize;
        (gx, gy)
    }
}

/// Build a normalized ring kernel (ri < r <= ra) in frequency domain.
fn build_ring_kernel_fft(ri: f32, ra: f32, planner: &mut FftPlanner<f32>) -> Vec<Complex<f32>> {
    let ri_sq = ri * ri;
    let ra_sq = ra * ra;
    let mut kernel = vec![Complex::new(0.0f32, 0.0); N];
    let mut count = 0.0f32;
    let rai = ra.ceil() as i32;

    for dy in -rai..=rai {
        for dx in -rai..=rai {
            let d_sq = (dx * dx + dy * dy) as f32;
            if d_sq > ri_sq && d_sq <= ra_sq {
                let x = (dx as isize).rem_euclid(GRID_W as isize) as usize;
                let y = (dy as isize).rem_euclid(GRID_H as isize) as usize;
                kernel[y * GRID_W + x].re += 1.0;
                count += 1.0;
            }
        }
    }

    if count > 0.0 {
        for v in &mut kernel {
            v.re /= count;
        }
    }

    fft_2d_forward(&mut kernel, planner);
    kernel
}

impl Renderer for SmoothLifeRenderer {
    fn render(&mut self, canvas: &Canvas, bounds: Rect) {
        if self.running {
            self.step();
        }

        self.update_pixels();

        canvas.clear(Color::BLACK);

        // Blit the grid as a raster image
        let info = ImageInfo::new(
            (GRID_W as i32, GRID_H as i32),
            ColorType::BGRA8888,
            AlphaType::Premul,
            None,
        );
        let row_bytes = GRID_W * 4;
        let pixel_bytes: &[u8] = bytemuck::cast_slice(&self.pixels);
        let data = Data::new_copy(pixel_bytes);
        if let Some(image) = skia_safe::images::raster_from_data(&info, data, row_bytes) {
            // Draw scaled to viewport
            let cs = self.cell_size as f32;
            let ox = self.offset_x as f32 * cs;
            let oy = self.offset_y as f32 * cs;
            let dst = Rect::from_xywh(-ox, -oy, GRID_W as f32 * cs, GRID_H as f32 * cs);
            canvas.draw_image_rect(
                &image,
                None,
                dst,
                &skia_safe::Paint::default(),
            );
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    fn handle_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
                self.drag = Some((*button, self.cursor_pos.0, self.cursor_pos.1));
                self.dragged = false;
            }
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                if !self.dragged {
                    // Paint a blob at click position
                    let (px, py) = self.cursor_pos;
                    let (gx, gy) = self.pixel_to_grid(px, py);
                    let r = self.params.ra as i32;
                    for dy in -r..=r {
                        for dx in -r..=r {
                            if dx * dx + dy * dy <= r * r {
                                let nx = (gx as i32 + dx).rem_euclid(GRID_W as i32) as usize;
                                let ny = (gy as i32 + dy).rem_euclid(GRID_H as i32) as usize;
                                self.grid[ny * GRID_W + nx] = 1.0;
                            }
                        }
                    }
                }
                self.drag = None;
            }
            WindowEvent::MouseInput { state: ElementState::Released, .. } => {
                self.drag = None;
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some((_, ref mut last_x, ref mut last_y)) = self.drag {
                    let dx = position.x - *last_x;
                    let dy = position.y - *last_y;
                    if dx.abs() > 2.0 || dy.abs() > 2.0 {
                        self.dragged = true;
                    }
                    if self.dragged {
                        self.offset_x -= dx / self.cell_size;
                        self.offset_y -= dy / self.cell_size;
                    }
                    *last_x = position.x;
                    *last_y = position.y;
                }
                self.cursor_pos = (position.x, position.y);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll_y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y as f64,
                    MouseScrollDelta::PixelDelta(pos) => pos.y / 50.0,
                };
                let factor = if scroll_y > 0.0 { 1.1 } else { 1.0 / 1.1 };
                let (cx, cy) = self.cursor_pos;
                let gx = cx / self.cell_size + self.offset_x;
                let gy = cy / self.cell_size + self.offset_y;
                self.cell_size = (self.cell_size * factor).clamp(1.0, 20.0);
                self.offset_x = gx - cx / self.cell_size;
                self.offset_y = gy - cy / self.cell_size;
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match &event.logical_key {
                    Key::Named(NamedKey::Space) => self.running = !self.running,
                    Key::Character(ch) if ch.as_str() == "s" => {
                        self.running = false;
                        self.step();
                    }
                    Key::Character(ch) if ch.as_str() == "r" => self.randomize(),
                    Key::Named(NamedKey::ArrowLeft)  => self.offset_x -= 5.0,
                    Key::Named(NamedKey::ArrowRight) => self.offset_x += 5.0,
                    Key::Named(NamedKey::ArrowUp)    => self.offset_y -= 5.0,
                    Key::Named(NamedKey::ArrowDown)  => self.offset_y += 5.0,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn egui_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.label("SmoothLife");
        ui.separator();

        if ui.button(if self.running { "⏸ Pause" } else { "▶ Play" }).clicked() {
            self.running = !self.running;
        }
        if ui.button("Step").clicked() {
            self.running = false;
            self.step();
        }
        if ui.button("Randomize").clicked() {
            self.randomize();
        }

        ui.separator();
        ui.label("Parameters");
        changed |= ui.add(egui::Slider::new(&mut self.params.ri, 1.0..=20.0).text("ri (inner)")).changed();
        changed |= ui.add(egui::Slider::new(&mut self.params.ra, 2.0..=30.0).text("ra (outer)")).changed();
        changed |= ui.add(egui::Slider::new(&mut self.params.b1, 0.0..=1.0).text("b1")).changed();
        changed |= ui.add(egui::Slider::new(&mut self.params.b2, 0.0..=1.0).text("b2")).changed();
        changed |= ui.add(egui::Slider::new(&mut self.params.d1, 0.0..=1.0).text("d1")).changed();
        changed |= ui.add(egui::Slider::new(&mut self.params.d2, 0.0..=1.0).text("d2")).changed();
        changed |= ui.add(egui::Slider::new(&mut self.params.alpha_n, 0.001..=0.5).text("α_n")).changed();
        changed |= ui.add(egui::Slider::new(&mut self.params.alpha_m, 0.001..=0.5).text("α_m")).changed();
        changed |= ui.add(egui::Slider::new(&mut self.params.dt, 0.01..=0.5).text("dt")).changed();

        ui.separator();
        ui.label("Click to paint, drag to pan");
        ui.label("Space: play/pause, S: step, R: randomize");
        changed
    }
}
