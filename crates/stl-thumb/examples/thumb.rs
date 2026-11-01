//! Dev tool: write the thumbnail of a local image, mesh or scene file, exactly as the cache
//! stores it; with `--spin`, a mesh's turntable strip instead; with `--all`, a 3MF's whole
//! carousel (`<out>.jpg`, then `<out>.p<i>.jpg`).
//!
//!   cargo run --release -p stl-thumb --example thumb -- [--spin|--all] <in.stl|obj|3mf|lys|png|jpg…> <out.jpg>
use stl_thumb::image::{encode_jpeg, image_thumb};
use stl_thumb::preview::{preview_bytes, threemf_pictures};
use stl_thumb::raster::{render, render_spin};
use stl_thumb::{ThumbKind, mesh::load_mesh, thumb_kind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let flag = args.first().filter(|a| a.starts_with("--")).cloned();
    if flag.is_some() {
        args.remove(0);
    }
    let spin = flag.as_deref() == Some("--spin");
    let [input, output] = args.as_slice() else {
        return Err("usage: thumb [--spin|--all] <input> <out.jpg>".into());
    };
    let bytes = std::fs::read(input)?;
    let t = std::time::Instant::now();
    if flag.as_deref() == Some("--all") {
        let (default, extras) = threemf_pictures(&bytes)?;
        let stem = output.strip_suffix(".jpg").unwrap_or(output);
        for (i, pic) in std::iter::once(&default).chain(&extras).enumerate() {
            let out = if i == 0 { output.clone() } else { format!("{stem}.p{i}.jpg") };
            let img = image_thumb(pic)?;
            std::fs::write(&out, encode_jpeg(&img)?)?;
            println!("{out}: {}×{}", img.width(), img.height());
        }
        println!("{} pictures after the default ({} ms)", extras.len(), t.elapsed().as_millis());
        return Ok(());
    }
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
