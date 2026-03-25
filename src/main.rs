mod application;
mod renderer;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let name = args.get(1).map(|s| s.as_str()).unwrap_or("mandelbrot");


    use renderer::Renderer;
    let renderer: Box<dyn Renderer> = match name {
        "icon" => {
            Box::new(renderer::icon_renderer::IconRenderer::new(60.0))
        }
        "mandelbrot" => {
            Box::new(renderer::mandelbrot::MandelbrotRenderer::new())
        }
        "julia" => {
            let preset = renderer::julia::PRESETS[0];
            Box::new(renderer::julia::JuliaRenderer::new(preset.0, preset.1))
        }
        "koch" => { Box::new(renderer::lsystem::koch::new(5)) }
        "sierpinski" => { Box::new(renderer::lsystem::sierpinski::new(5)) }
        "dragon" => { Box::new(renderer::lsystem::dragon::new(12)) }
        "barnsley" | "barnsley_fern" | "fern" => { Box::new(renderer::lsystem::barnsley_fern::new(5)) }
        "life" | "game_of_life" | "gol" => { Box::new(renderer::game_of_life::GameOfLifeRenderer::new()) }
        "smoothlife" | "smooth" => { Box::new(renderer::smooth_life::SmoothLifeRenderer::new()) }
        _ => panic!("unknown renderer {name}")
    };

    let el = application::create_event_loop();
    let mut app = application::create_application(&el, renderer);

    el.run_app(&mut app).expect("run() failed");
}
