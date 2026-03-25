use super::Renderer;
use skia_safe::{Canvas, Color, Paint, Rect};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key, NamedKey};
use std::collections::{HashMap, HashSet};

const DEFAULT_CELL_SIZE: f64 = 12.0;

pub struct GameOfLifeRenderer {
    /// Living cells as (col, row) in infinite grid coordinates
    cells: HashSet<(i64, i64)>,
    /// Viewport offset in grid units (top-left corner)
    offset_x: f64,
    offset_y: f64,
    /// Pixels per cell
    cell_size: f64,
    width: u32,
    height: u32,
    running: bool,
    cursor_pos: (f64, f64),
    /// Drag tracking: Some((button, last_x, last_y))
    drag: Option<(MouseButton, f64, f64)>,
    /// Whether mouse moved during current press (click vs drag)
    dragged: bool,
}

impl GameOfLifeRenderer {
    pub fn new() -> Self {
        let mut s = Self {
            cells: HashSet::new(),
            offset_x: -20.0,
            offset_y: -20.0,
            cell_size: DEFAULT_CELL_SIZE,
            width: 0,
            height: 0,
            running: false,
            cursor_pos: (0.0, 0.0),
            drag: None,
            dragged: false,
        };
        // Glider
        for &(c, r) in &[(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)] {
            s.cells.insert((c, r));
        }
        s
    }

    fn step(&mut self) {
        let mut neighbor_counts: HashMap<(i64, i64), u8> = HashMap::new();
        for &(cx, cy) in &self.cells {
            for dx in -1..=1i64 {
                for dy in -1..=1i64 {
                    if dx == 0 && dy == 0 { continue; }
                    *neighbor_counts.entry((cx + dx, cy + dy)).or_default() += 1;
                }
            }
        }
        let old = &self.cells;
        self.cells = neighbor_counts
            .into_iter()
            .filter(|&(pos, n)| n == 3 || (n == 2 && old.contains(&pos)))
            .map(|(pos, _)| pos)
            .collect();
    }

    fn pixel_to_cell(&self, px: f64, py: f64) -> (i64, i64) {
        let col = (px / self.cell_size + self.offset_x).floor() as i64;
        let row = (py / self.cell_size + self.offset_y).floor() as i64;
        (col, row)
    }
}

impl Renderer for GameOfLifeRenderer {
    fn render(&mut self, canvas: &Canvas, bounds: Rect) {
        if self.running {
            self.step();
        }

        canvas.clear(Color::from_rgb(24, 24, 24));

        let cs = self.cell_size as f32;
        let cols = (bounds.width() as f64 / self.cell_size).ceil() as i64 + 1;
        let rows = (bounds.height() as f64 / self.cell_size).ceil() as i64 + 1;
        let start_col = self.offset_x.floor() as i64;
        let start_row = self.offset_y.floor() as i64;
        let frac_x = (self.offset_x - self.offset_x.floor()) as f32 * cs;
        let frac_y = (self.offset_y - self.offset_y.floor()) as f32 * cs;

        // Grid lines
        let mut grid_paint = Paint::default();
        grid_paint.set_color(Color::from_rgb(40, 40, 40));
        grid_paint.set_anti_alias(false);
        for c in 0..=cols {
            let x = c as f32 * cs - frac_x;
            canvas.draw_line((x, bounds.top), (x, bounds.bottom), &grid_paint);
        }
        for r in 0..=rows {
            let y = r as f32 * cs - frac_y;
            canvas.draw_line((bounds.left, y), (bounds.right, y), &grid_paint);
        }

        // Living cells
        let mut cell_paint = Paint::default();
        cell_paint.set_color(Color::from_rgb(100, 255, 100));
        cell_paint.set_anti_alias(false);
        for &(cx, cy) in &self.cells {
            let sc = cx - start_col;
            let sr = cy - start_row;
            if sc >= 0 && sc < cols && sr >= 0 && sr < rows {
                let x = sc as f32 * cs - frac_x + 1.0;
                let y = sr as f32 * cs - frac_y + 1.0;
                canvas.draw_rect(Rect::from_xywh(x, y, cs - 2.0, cs - 2.0), &cell_paint);
            }
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
                    let (px, py) = self.cursor_pos;
                    let cell = self.pixel_to_cell(px, py);
                    if !self.cells.remove(&cell) {
                        self.cells.insert(cell);
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
                // Zoom centered on cursor
                let (cx, cy) = self.cursor_pos;
                let gx = cx / self.cell_size + self.offset_x;
                let gy = cy / self.cell_size + self.offset_y;
                self.cell_size = (self.cell_size * factor).clamp(2.0, 100.0);
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
                    Key::Character(ch) if ch.as_str() == "r" => {
                        self.cells.clear();
                    }
                    Key::Named(NamedKey::ArrowLeft)  => self.offset_x -= 3.0,
                    Key::Named(NamedKey::ArrowRight) => self.offset_x += 3.0,
                    Key::Named(NamedKey::ArrowUp)    => self.offset_y -= 3.0,
                    Key::Named(NamedKey::ArrowDown)  => self.offset_y += 3.0,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn egui_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.label("Game of Life");
        ui.separator();

        if ui.button(if self.running { "⏸ Pause" } else { "▶ Play" }).clicked() {
            self.running = !self.running;
            changed = true;
        }
        if ui.button("Step").clicked() {
            self.running = false;
            self.step();
            changed = true;
        }
        if ui.button("Clear").clicked() {
            self.cells.clear();
            changed = true;
        }
        ui.separator();
        ui.label(format!("Living cells: {}", self.cells.len()));
        ui.label("Click to toggle cells");
        ui.label("Drag to pan, scroll to zoom");
        ui.label("Space: play/pause, S: step, R: clear");
        changed
    }
}
