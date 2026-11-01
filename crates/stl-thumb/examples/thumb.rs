//! Dev tool: write the thumbnail of a local image, mesh or scene file, exactly as the cache
//! stores it; with `--spin`, a mesh's turntable strip instead.
//!
//!   cargo run --release -p stl-thumb --example thumb -- [--spin] <in.stl|obj|3mf|lys|png|jpg…> <out.jpg>
use stl_thumb::image::{encode_jpeg, image_thumb};
use stl_thumb::preview::preview_bytes;
use stl_thumb::raster::{render, render_spin};
use stl_thumb::{ThumbKind, mesh::load_mesh, thumb_kind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let spin = args.first().is_some_and(|a| a == "--spin");
    if spin {
        args.remove(0);
    }
    let [input, output] = args.as_slice() else {
        return Err("usage: thumb [--spin] <input> <out.jpg>".into());
    };
    let bytes = std::fs::read(input)?;
    let t = std::time::Instant::now();
    let img = match thumb_kind(input).ok_or("no thumbnail for this file type")? {
        ThumbKind::Mesh if spin => render_spin(&load_mesh(input, &bytes)?),
        _ if spin => return Err("only meshes have a turntable strip".into()),
        ThumbKind::Image => image_thumb(&bytes)?,
        ThumbKind::Mesh => render(&load_mesh(input, &bytes)?),
        ThumbKind::Preview => image_thumb(&preview_bytes(input, &bytes)?)?,
    };
    let jpg = encode_jpeg(&img)?;
    std::fs::write(output, &jpg)?;
    println!("{output}: {}×{}, {} bytes ({} ms)", img.width(), img.height(), jpg.len(), t.elapsed().as_millis());
    Ok(())
}
