mod renderer;
mod application;

fn main() {
    let el = application::create_event_loop();
    let mut application = application::create_application(&el);
    el.run_app(&mut application).expect("run() failed");
}
