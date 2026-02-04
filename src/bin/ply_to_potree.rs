use std::path::PathBuf;

use clap::Parser;
use potree::convert::ply_loader::load_ply_positions;
use potree::convert::build_potree_buffers;

/// Convert a PLY file into Potree format (octree.bin, hierarchy.bin, metadata.json)
#[derive(Debug, Parser)]
struct Args {
    /// Input PLY file
    input: PathBuf,

    /// Output directory to write Potree files into
    output: PathBuf,

    /// Optional pointcloud name (defaults to input file stem)
    #[arg(long)]
    name: Option<String>,

    /// Projection string to place in metadata.json
    #[arg(long)]
    projection: Option<String>,

    /// Uniform scale factor to apply to coordinates
    #[arg(long, default_value_t = 1.0)]
    scale: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let input = args.input;
    let output = args.output;

    let data = load_ply_positions(&input)?;

    let scale_arr = [args.scale, args.scale, args.scale];

    let name = args.name.unwrap_or_else(|| {
        input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("pointcloud")
            .to_string()
    });

    std::fs::create_dir_all(&output)?;

    let buffers = build_potree_buffers(
        &name,
        &args.projection.unwrap_or_default(),
        &data.positions,
        data.colors.as_deref(),
        scale_arr,
        "DEFAULT",
    )?;

    let octree_path = output.join("octree.bin");
    let hierarchy_path = output.join("hierarchy.bin");
    let metadata_path = output.join("metadata.json");

    std::fs::write(&octree_path, &buffers.octree)?;
    std::fs::write(&hierarchy_path, &buffers.hierarchy)?;
    std::fs::write(&metadata_path, &buffers.metadata_json)?;

    println!("Wrote Potree output to {}", output.display());

    Ok(())
}
