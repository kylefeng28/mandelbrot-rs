/// SmoothLife: a continuous generalization of Conway's Game of Life.
/// Stephan Rafler, 2011 — "Generalization of Conway's Game of Life to a continuous domain"
///
/// Instead of a discrete grid with {0,1} states, cells hold f32 values in [0,1].
/// Neighborhoods are concentric discs with smooth transition functions.
use super::Renderer;
use skia_safe::{
    AlphaType, Canvas, Color, ColorType, Data, ImageInfo, Rect,
};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key, NamedKey};

/// Grid dimensions for the simulation
const GRID_W: usize = 256;
const GRID_H: usize = 256;

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

impl SmoothLifeRenderer {
    pub fn new() -> Self {
        let mut grid = vec![0.0f32; GRID_W * GRID_H];
        // Seed: random blob in center
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

        Self {
            grid,
            pixels: vec![0; GRID_W * GRID_H],
            params: Params::default(),
            running: true,
            offset_x: 0.0,
            offset_y: 0.0,
            cell_size: 3.0,
            width: 0,
            height: 0,
            cursor_pos: (0.0, 0.0),
            drag: None,
            dragged: false,
        }
    }

    fn step(&mut self) {
        let w = GRID_W;
        let h = GRID_H;
        let p = &self.params;
        let ra = p.ra;
        let ri = p.ri;
        let ra_i = ra.ceil() as i32;

        // Precompute disc areas
        let ri_sq = ri * ri;
        let ra_sq = ra * ra;

        let mut new_grid = vec![0.0f32; w * h];

        for cy in 0..h {
            for cx in 0..w {
                let mut m_sum = 0.0f32; // inner disc
                let mut m_count = 0.0f32;
                let mut n_sum = 0.0f32; // outer ring
                let mut n_count = 0.0f32;

                for dy in -ra_i..=ra_i {
                    for dx in -ra_i..=ra_i {
                        let dist_sq = (dx * dx + dy * dy) as f32;
                        let nx = (cx as i32 + dx).rem_euclid(w as i32) as usize;
                        let ny = (cy as i32 + dy).rem_euclid(h as i32) as usize;
                        let val = self.grid[ny * w + nx];

                        if dist_sq <= ri_sq {
                            m_sum += val;
                            m_count += 1.0;
                        } else if dist_sq <= ra_sq {
                            n_sum += val;
                            n_count += 1.0;
                        }
                    }
                }

                let m = if m_count > 0.0 { m_sum / m_count } else { 0.0 };
                let n = if n_count > 0.0 { n_sum / n_count } else { 0.0 };

                let s = transition(n, m, p);
                let cur = self.grid[cy * w + cx];
                new_grid[cy * w + cx] = (cur + p.dt * (2.0 * s - 1.0)).clamp(0.0, 1.0);
            }
        }

        self.grid = new_grid;
    }

    fn update_pixels(&mut self) {
        for (i, &v) in self.grid.iter().enumerate() {
            // Color map: black (0) → blue → cyan → white (1)
            let t = v.clamp(0.0, 1.0);
            let r: u32;
            let g: u32;
            let b: u32;
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
