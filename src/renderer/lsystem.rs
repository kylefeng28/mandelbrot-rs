use super::{
    Renderer,
    viewer::{Draggable, DragState, DragEvent, PanOrZoom},
};
use skia_safe::{Canvas, Color, Paint, PaintStyle, PathBuilder, Rect};
use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{Key, NamedKey};

/// An L-system grammar is defined by an axiom (starting string) and production rules.
/// Starting from the axiom, we apply each rule by replacing the from_char with the replacement
/// string, and repeat this until the desired number of iterations is reached.
///
/// For example, a Koch snowflake can be defined with this L-system grammar:
///   Axiom: F--F--F (equilateral triangle)
///   Rule:  F -> F+F--F+F
///   Angle: 60°
/// Iteration 0: F--F--F
/// Iteration 1: (F+F--F+F)--(F+F--F+F)--(F+F--F+F)
/// Iteration 2: ((F+F--F+F)+(F+F--F+F)--(F+F--F+F)+(F+F--F+F))--((F+F--F+F)+(F+F--F+F)--(F+F--F+F)+(F+F--F+F))--((F+F--F+F)+(F+F--F+F)--(F+F--F+F)+(F+F--F+F))
/// ...
pub struct LSystemDef {
    /// Starting string
    pub axiom: &'static str,
    /// Production rules: (from_char, replacement_string)
    pub rules: &'static [(char, &'static str)],
    /// Turn angle in degrees
    pub angle: f64,
    /// Initial heading in degrees (0 = right, 90 = up)
    pub initial_heading: f64,
}

impl LSystemDef {
    fn expand(&self, iterations: u32) -> String {
        let mut current = self.axiom.to_string();
        for _ in 0..iterations {
            let mut next = String::with_capacity(current.len() * 4);
            for ch in current.chars() {
                let replaced = self.rules.iter()
                    .find(|(from, _)| *from == ch)
                    .map(|(_, to)| *to);
                match replaced {
                    Some(s) => next.push_str(s),
                    None => next.push(ch),
                }
            }
            current = next;
        }
        current
    }

    /// Interpret the L-system string as turtle graphics commands
    /// Returns a list of line segments as ((x1,y1), (x2,y2))
    fn to_segments(&self, instructions: &str) -> Vec<((f64, f64), (f64, f64))> {
        let mut segments = Vec::new();
        let mut x = 0.0_f64;
        let mut y = 0.0_f64;
        let mut heading = self.initial_heading.to_radians();
        let angle_rad = self.angle.to_radians();
        let mut stack: Vec<(f64, f64, f64)> = Vec::new();

        for ch in instructions.chars() {
            match ch {
                // Draw forward
                'F' => {
                    let nx = x + heading.cos();
                    let ny = y + heading.sin();
                    segments.push(((x, y), (nx, ny)));
                    x = nx;
                    y = ny;
                }
                // Move forward without drawing
                'f' => {
                    x += heading.cos();
                    y += heading.sin();
                }
                // Turn left
                '+' => heading += angle_rad,
                // Turn right
                '-' => heading -= angle_rad,
                // Push state
                '[' => stack.push((x, y, heading)),
                // Pop state
                ']' => {
                    if let Some((sx, sy, sh)) = stack.pop() {
                        x = sx;
                        y = sy;
                        heading = sh;
                    }
                }
                _ => {}
            }
        }
        segments
    }
}

/// A interactive renderer for an L-system
/// Press [ and ] to decrease/increase iteration depth
/// Press 'r' to reset the viewport
pub struct LSystemRenderer {
    def: LSystemDef,
    iterations: u32,
    max_iterations: u32,
    /// Cached segments from the last expansion
    segments: Vec<((f64, f64), (f64, f64))>,
    /// Viewport: center and scale (units visible from center to edge)
    center_x: f64,
    center_y: f64,
    scale: f64,
    width: u32,
    height: u32,
    dirty: bool,
    drag_state: DragState,
    cursor_pos: (f64, f64),
    stroke_color: Color,
}

impl LSystemRenderer {
    pub fn new(def: LSystemDef, initial_iterations: u32, max_iterations: u32) -> Self {
        let mut r = Self {
            def,
            iterations: initial_iterations,
            max_iterations,
            segments: Vec::new(),
            center_x: 0.0,
            center_y: 0.0,
            scale: 1.0,
            width: 0,
            height: 0,
            dirty: true,
            drag_state: DragState::None,
            cursor_pos: (0.0, 0.0),
            stroke_color: Color::from(0xff_1a73e8), // Chromium blue
        };
        r.recompute();
        r.fit_to_bounds();
        r
    }

    pub fn set_stroke_color(&mut self, color: Color) {
        self.stroke_color = color;
    }

    /// Recompute segments from the L-system definition
    fn recompute(&mut self) {
        let instructions = self.def.expand(self.iterations);
        self.segments = self.def.to_segments(&instructions);
    }

    /// Auto-fit the viewport to show all segments
    fn fit_to_bounds(&mut self) {
        if self.segments.is_empty() {
            return;
        }
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        for ((x1, y1), (x2, y2)) in &self.segments {
            min_x = min_x.min(*x1).min(*x2);
            min_y = min_y.min(*y1).min(*y2);
            max_x = max_x.max(*x1).max(*x2);
            max_y = max_y.max(*y1).max(*y2);
        }
        self.center_x = (min_x + max_x) / 2.0;
        self.center_y = (min_y + max_y) / 2.0;
        let w = max_x - min_x;
        let h = max_y - min_y;
        // Add 10% padding
        self.scale = w.max(h) * 0.55;
    }
}

impl Renderer for LSystemRenderer {
    fn render(&mut self, canvas: &Canvas, bounds: Rect) {
        let w = bounds.width();
        let h = bounds.height();
        self.width = w as u32;
        self.height = h as u32;

        if self.dirty {
            self.recompute();
            self.dirty = false;
        }

        if self.segments.is_empty() {
            return;
        }

        let aspect = w as f64 / h.max(1.0) as f64;
        let half_w = self.scale * aspect;
        let half_h = self.scale;

        // Build a Skia path from all segments
        let mut path = PathBuilder::new();
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Stroke);
        paint.set_color(self.stroke_color);
        // Scale stroke width so it looks consistent at any zoom
        paint.set_stroke_width((1.0 / self.segments.len() as f32).max(0.5).min(2.0));

        for ((x1, y1), (x2, y2)) in &self.segments {
            // Map from L-system coords to screen coords
            let sx1 = bounds.left + ((x1 - self.center_x + half_w) / (2.0 * half_w) * w as f64) as f32;
            let sy1 = bounds.top + ((self.center_y + half_h - y1) / (2.0 * half_h) * h as f64) as f32;
            let sx2 = bounds.left + ((x2 - self.center_x + half_w) / (2.0 * half_w) * w as f64) as f32;
            let sy2 = bounds.top + ((self.center_y + half_h - y2) / (2.0 * half_h) * h as f64) as f32;
            path.move_to((sx1, sy1));
            path.line_to((sx2, sy2));
        }

        canvas.draw_path(&path.detach(), &paint);
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    fn handle_event(&mut self, event: &WindowEvent) {
        // Handle iteration depth changes with [ and ]
        if let WindowEvent::KeyboardInput { event: key_event, .. } = event {
            if key_event.state == ElementState::Pressed {
                match &key_event.logical_key {
                    Key::Character(ch) if ch.as_str() == "[" => {
                        if self.iterations > 0 {
                            self.iterations -= 1;
                            self.dirty = true;
                        }
                    }
                    Key::Character(ch) if ch.as_str() == "]" => {
                        if self.iterations < self.max_iterations {
                            self.iterations += 1;
                            self.dirty = true;
                        }
                    }
                    // Reset view to fit all segments
                    Key::Character(ch) if ch.as_str() == "r" => {
                        self.fit_to_bounds();
                    }
                    _ => {}
                }
            }
        }

        let drag_event = self.handle_drag_event(
            event, self.width, self.height, self.scale,
        );
        match drag_event {
            DragEvent::Move(dx, dy) | DragEvent::Drag(_, dx, dy) => {
                self.center_x += dx;
                self.center_y += dy;
            }
            DragEvent::Zoom(factor) => {
                self.scale *= factor;
            }
            DragEvent::None => {}
        }
    }
}

impl Draggable for LSystemRenderer {
    fn set_cursor_pos(&mut self, x: f64, y: f64) { self.cursor_pos = (x, y); }
    fn get_cursor_pos(&mut self) -> (f64, f64) { self.cursor_pos }
    fn set_drag_state(&mut self, drag_state: DragState) { self.drag_state = drag_state; }
    fn get_drag_state(&self) -> &DragState { &self.drag_state }
}

pub mod koch {
    use super::{LSystemDef, LSystemRenderer};

    /// Koch snowflake L-system
    ///
    /// Axiom: F--F--F (equilateral triangle)
    /// Rule:  F -> F+F--F+F
    /// Angle: 60°
    ///
    /// Press [ and ] to decrease/increase iteration depth
    /// Press 'r' to reset the viewport
    pub fn new(iterations: u32) -> LSystemRenderer {
        LSystemRenderer::new(
            LSystemDef {
                axiom: "F--F--F",
                rules: &[('F', "F+F--F+F")],
                angle: 60.0,
                initial_heading: 0.0,
            },
            iterations,
            7, // max iterations (gets very dense beyond this)
        )
    }
}
