use winit::event::{ElementState, WindowEvent, MouseButton, MouseScrollDelta};
use winit::keyboard::{Key, NamedKey};

pub enum DragState {
    None,
    /// State indicating the left mouse button is currently held for dragging
    /// Holds coordinates of the last processed cursor coordiantes (in pixels)
    Dragging(f64, f64),
}

#[derive(Debug)]
pub enum PanOrZoom {
    None,
    Pan(f64, f64),
    Zoom(f64), // scale
}

pub trait Draggable {
    fn handle_drag_event(&mut self, event: &WindowEvent, width: u32, height: u32, scale: f64) -> PanOrZoom {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return PanOrZoom::None;
                }
                let pan_amount = scale * 0.1;
                match &event.logical_key {
                    // Pan with arrow keys
                    Key::Named(NamedKey::ArrowLeft)  => { PanOrZoom::Pan(-pan_amount, 0.0) }
                    Key::Named(NamedKey::ArrowRight) => { PanOrZoom::Pan( pan_amount, 0.0) }
                    Key::Named(NamedKey::ArrowUp)    => { PanOrZoom::Pan(0.0,  pan_amount) }
                    Key::Named(NamedKey::ArrowDown)  => { PanOrZoom::Pan(0.0, -pan_amount) }

                    // Zoom with +/-
                    Key::Character(ch) if ch.as_str() == "=" || ch.as_str() == "+" => {
                        PanOrZoom::Zoom(0.8)
                    }
                    Key::Character(ch) if ch.as_str() == "-" => {
                        PanOrZoom::Zoom(1.25)
                    }
                    _ => PanOrZoom::None
                }
            }
            // Drag to pan: mouse down starts drag
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                if *state == ElementState::Pressed {
                    let (cursor_x, cursor_y) = self.get_cursor_pos();
                    self.set_drag_state(DragState::Dragging(cursor_x, cursor_y));
                } else {
                    self.set_drag_state(DragState::None);
                }
                PanOrZoom::None
            }
            // Drag to pan: mouse move while drag_state
            WindowEvent::CursorMoved { position, .. } => {
                if let DragState::Dragging(drag_x, drag_y) = self.get_drag_state() && width > 0 {
                    let dx = position.x - drag_x;
                    let dy = position.y - drag_y;

                    // Convert pixel delta to complex plane delta
                    let aspect = width as f64 / height.max(1) as f64;
                    let pixels_per_unit_x = width as f64 / (2.0 * scale * aspect);
                    let pixels_per_unit_y = height as f64 / (2.0 * scale);

                    self.set_drag_state(DragState::Dragging(position.x, position.y));
                    return PanOrZoom::Pan(-dx / pixels_per_unit_x, dy / pixels_per_unit_y);
                }

                self.set_cursor_pos(position.x, position.y);
                PanOrZoom::None
            }
            // Scroll to zoom (centered on cursor position)
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll_y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y as f64,
                    MouseScrollDelta::PixelDelta(pos) => pos.y / 50.0,
                };
                // Zoom factor: scroll up zooms in, scroll down zooms out
                let factor = if scroll_y > 0.0 { 0.9 } else { 1.0 / 0.9 };
                PanOrZoom::Zoom(factor)
            }
            _ => PanOrZoom::None
        }
    }

    fn set_cursor_pos(&mut self, x: f64, y: f64);
    fn get_cursor_pos(&mut self) -> (f64, f64);
    fn set_drag_state(&mut self, drag_state: DragState);
    fn get_drag_state(&self) -> &DragState;
}
