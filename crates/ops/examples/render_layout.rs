//! Render an image through a layout and write the screen frame as a PNG:
//! `cargo run -p receiverproxy-ops --example render_layout -- layout.json in.png out.png`
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let canvas: wall::Canvas = serde_json::from_str(&std::fs::read_to_string(&a[1])?)?;
    let img = image::open(&a[2])?;
    let frame = ops::display::image_frame(&img, &canvas, sources::Fit::Stretch)?;
    let screen = canvas.render(&frame);
    let out = image::RgbImage::from_raw(screen.width, screen.height, screen.as_bytes().to_vec())
        .ok_or_else(|| anyhow::anyhow!("size"))?;
    out.save(&a[3])?;
    println!(
        "{}x{} -> {}x{}",
        frame.width, frame.height, screen.width, screen.height
    );
    Ok(())
}
