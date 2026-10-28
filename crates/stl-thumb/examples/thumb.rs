//! Dev tool: write the thumbnail of a local image or mesh file, exactly as the cache stores it.
//!
//!   cargo run --release -p stl-thumb --example thumb -- <in.stl|obj|png|jpg…> <out.jpg>
use stl_thumb::image::{encode_jpeg, image_thumb};
use stl_thumb::{ThumbKind, mesh::load_mesh, raster::render, thumb_kind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        return Err("usage: thumb <input> <out.jpg>".into());
    };
    let bytes = std::fs::read(input)?;
    let t = std::time::Instant::now();
    let img = match thumb_kind(input).ok_or("no thumbnail for this file type")? {
        ThumbKind::Image => image_thumb(&bytes)?,
        ThumbKind::Mesh => render(&load_mesh(input, &bytes)?),
    };
    let jpg = encode_jpeg(&img)?;
    std::fs::write(output, &jpg)?;
    println!("{output}: {}×{}, {} bytes ({} ms)", img.width(), img.height(), jpg.len(), t.elapsed().as_millis());
    Ok(())
}
