mod application;
mod renderer;

fn main() {
    let el = application::create_event_loop();
    // let renderer = Box::new(renderer::icon_renderer::IconRenderer::new(60.0));
    let renderer = Box::new(renderer::mandelbrot::MandelbrotRenderer::new());
    let mut app = application::create_application(&el, renderer);

    el.run_app(&mut app).expect("run() failed");
}
