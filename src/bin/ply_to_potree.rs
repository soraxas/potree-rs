use std::path::PathBuf;

use clap::Parser;
use potree::convert::ply_loader::load_ply_positions;
use potree::convert::{
    compute_scale_offset, estimate_spacing, write_metadata_json, write_octree_bin_positions,
    write_single_root_hierarchy,
};

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
    #[arg(long, default_value_t = 0.001)]
    scale: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let input = args.input;
    let output = args.output;

    let data = load_ply_positions(&input)?;
    if data.positions.is_empty() {
        return Err("PLY contains no vertices".into());
    }

    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];

    for p in &data.positions {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }

    let scale_arr = [args.scale, args.scale, args.scale];
    let (scale, offset) = compute_scale_offset(min, max, scale_arr);
    let points = data.positions.len() as u64;
    let spacing = estimate_spacing(min, max, points);

    let name = args.name.unwrap_or_else(|| {
        input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("pointcloud")
            .to_string()
    });

    std::fs::create_dir_all(&output)?;

    let octree_path = output.join("octree.bin");
    let hierarchy_path = output.join("hierarchy.bin");
    let metadata_path = output.join("metadata.json");

    let byte_size = write_octree_bin_positions(
        &octree_path,
        &data.positions,
        data.colors.as_deref(),
        scale,
        offset,
    )?;
    write_single_root_hierarchy(&hierarchy_path, points as u32, byte_size)?;
    write_metadata_json(
        &metadata_path,
        &name,
        &args.projection.unwrap_or_default(),
        points,
        min,
        max,
        scale,
        offset,
        spacing,
        "DEFAULT",
        data.colors.is_some(),
    )?;

    println!("Wrote Potree output to {}", output.display());

    Ok(())
}
