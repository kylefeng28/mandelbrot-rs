## mandelbrot-rs

Mandelbrot viewer written in Rust, using [Skia](https://skia.org/docs/) as the rendering engine and [skia-safe bindings for Rust](https://github.com/rust-skia/rust-skia)

- Mandelbrot: $z_0 = 0$, iterate $z = z^2 + c$ where $c$ varies per pixel
- Julia: $c$ is fixed, iterate $z = z^2 + c$ where $z_0$ varies per pixel

**Mandelbrot**
![](./screenshots/mandelbrot.png)

**Julia Set**
![](./screenshots/julia_1.png)
![](./screenshots/julia_2.png)
![](./screenshots/julia_3.png)
![](./screenshots/julia_4.png)

**Sierpinski Triangle**
![](./screenshots/sierpinski.png)
**Koch Snowflake**
![](./screenshots/koch_snowflake.png)
**Barnsley Fern**
![](./screenshots/barnsley_fern.png)
