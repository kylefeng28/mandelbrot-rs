use winit::event::{ElementState, WindowEvent, MouseButton, MouseScrollDelta};
use winit::keyboard::{Key, NamedKey};

pub enum DragState {
    None,
    /// State indicating the left mouse button is currently held for dragging
    /// Holds coordinates of the last processed cursor coordinates (in pixels)
    Dragging(MouseButton, f64, f64),
}

#[derive(Debug)]
pub enum DragEvent {
    None,
    Move(f64, f64),
    Drag(MouseButton, f64, f64),
    Zoom(f64),
}

#[derive(Debug)]
pub enum PanOrZoom {
    None,
    Pan(f64, f64),
    Zoom(f64), // scale
}

pub trait Draggable {
    fn handle_drag_event(&mut self, event: &WindowEvent, width: u32, height: u32, scale: f64) -> DragEvent {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return DragEvent::None;
                }
                let pan_amount = scale * 0.1;
                match &event.logical_key {
                    // Pan with arrow keys
                    Key::Named(NamedKey::ArrowLeft)  => { DragEvent::Move(-pan_amount, 0.0) }
                    Key::Named(NamedKey::ArrowRight) => { DragEvent::Move( pan_amount, 0.0) }
                    Key::Named(NamedKey::ArrowUp)    => { DragEvent::Move(0.0,  pan_amount) }
                    Key::Named(NamedKey::ArrowDown)  => { DragEvent::Move(0.0, -pan_amount) }

                    // Zoom with +/-
                    Key::Character(ch) if ch.as_str() == "=" || ch.as_str() == "+" => {
                        DragEvent::Zoom(0.8)
                    }
                    Key::Character(ch) if ch.as_str() == "-" => {
                        DragEvent::Zoom(1.25)
                    }
                    _ => DragEvent::None
                }
            }
            // Drag to pan: mouse down starts drag
            WindowEvent::MouseInput { state, button, .. } => {
                if *state == ElementState::Pressed {
                    let (cursor_x, cursor_y) = self.get_cursor_pos();
                    self.set_drag_state(DragState::Dragging(*button, cursor_x, cursor_y));
                } else {
                    self.set_drag_state(DragState::None);
                }
                DragEvent::None
            }
            // Drag to pan: mouse move while drag_state
            WindowEvent::CursorMoved { position, .. } => {
                // Copy values out of the borrow before mutating self
                if let &DragState::Dragging(drag_btn, drag_x, drag_y) = self.get_drag_state() && width > 0 {
                    let dx = position.x - drag_x;
                    let dy = position.y - drag_y;

                    // Convert pixel delta to complex plane delta
                    let aspect = width as f64 / height.max(1) as f64;
                    let pixels_per_unit_x = width as f64 / (2.0 * scale * aspect);
                    let pixels_per_unit_y = height as f64 / (2.0 * scale);

                    self.set_drag_state(DragState::Dragging(drag_btn, position.x, position.y));
                    return DragEvent::Drag(drag_btn, -dx / pixels_per_unit_x, dy / pixels_per_unit_y);
                }

                self.set_cursor_pos(position.x, position.y);
                DragEvent::None
            }
            // Scroll to zoom (centered on cursor position)
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll_y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y as f64,
                    MouseScrollDelta::PixelDelta(pos) => pos.y / 50.0,
                };
                // Zoom factor: scroll up zooms in, scroll down zooms out
                let factor = if scroll_y > 0.0 { 0.9 } else { 1.0 / 0.9 };
                DragEvent::Zoom(factor)
            }
            _ => DragEvent::None
        }
    }

    fn set_cursor_pos(&mut self, x: f64, y: f64);
    fn get_cursor_pos(&mut self) -> (f64, f64);
    fn set_drag_state(&mut self, drag_state: DragState);
    fn get_drag_state(&self) -> &DragState;
}
