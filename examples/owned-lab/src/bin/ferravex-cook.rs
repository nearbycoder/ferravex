//! Offline importer and packer; no runtime source-file parsing is required for cooked meshes.
use ferravex_engine::{assets::Bundle, audio::Clip};
use ferravex_graphics::{Color, Mesh};
use std::{
    io::{self, Read},
    path::Path,
};
fn cook() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err(
            "Usage: ferravex-cook OUTPUT.ferravexpak INPUT.obj|INPUT.wav|INPUT.bin ...".into(),
        );
    }
    let mut bundle = Bundle::default();
    for file in &args[1..] {
        let path = Path::new(file);
        let mut data = Vec::new();
        std::fs::File::open(path)?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut data)?;
        if data.len() > 64 * 1024 * 1024 {
            return Err("input exceeds 64 MiB".into());
        }
        let mut name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("invalid file name")?
            .to_string();
        match path.extension().and_then(|s| s.to_str()) {
            Some("obj") => {
                let mesh = Mesh::from_obj(std::str::from_utf8(&data)?, Color::from_hex(0xcabda2))?;
                data = mesh.to_cooked()?;
                name = path
                    .with_extension("gmesh")
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string();
            }
            Some("wav") => {
                Clip::wav(&data).ok_or("expected bounded PCM16 mono/stereo WAV")?;
            }
            _ => {}
        }
        if bundle.get(&name).is_some() {
            return Err(format!("duplicate output asset: {name}").into());
        }
        bundle
            .insert(&name, &data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{e:?}")))?;
    }
    let bytes = bundle.encode();
    std::fs::write(&args[0], &bytes)?;
    println!(
        "Cooked {} files into {} bytes: {}",
        args.len() - 1,
        bytes.len(),
        args[0]
    );
    Ok(())
}
fn main() {
    if let Err(error) = cook() {
        eprintln!("{error}");
        std::process::exit(1)
    }
}
