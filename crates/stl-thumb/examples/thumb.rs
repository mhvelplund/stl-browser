//! Dev tool: write the thumbnail of a local image or mesh file as PNG.
//!
//!   cargo run --release -p stl-thumb --example thumb -- <in.stl|obj|png|jpg…> <out.png>
use stl_thumb::{ThumbKind, image::image_thumb, mesh::load_mesh, raster::render, thumb_kind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        return Err("usage: thumb <input> <out.png>".into());
    };
    let bytes = std::fs::read(input)?;
    let t = std::time::Instant::now();
    let img = match thumb_kind(input).ok_or("no thumbnail for this file type")? {
        ThumbKind::Image => image_thumb(&bytes)?,
        ThumbKind::Mesh => render(&load_mesh(input, &bytes)?),
    };
    img.save(output)?;
    println!("{output} ({} ms)", t.elapsed().as_millis());
    Ok(())
}
