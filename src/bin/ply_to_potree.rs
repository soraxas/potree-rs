use std::path::PathBuf;

use clap::Parser;
use potree::convert::ply_loader::load_ply_positions;

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

    /// Maximum number of points to keep in each node (before splitting or sampling)
    #[arg(long, default_value_t = 100_000)]
    max_points_per_node: usize,

    /// Maximum octree depth
    #[arg(long, default_value_t = 20)]
    max_depth: u32,

    /// Optional RNG seed for reproducible sampling
    #[arg(long)]
    seed: Option<u64>,

}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let input = args.input;
    let output = args.output;

    let scale_arr = [args.scale, args.scale, args.scale];

    let name = args.name.unwrap_or_else(|| {
        input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("pointcloud")
            .to_string()
    });

    std::fs::create_dir_all(&output)?;

    let data = load_ply_positions(&input)?;
    let mut builder = data
        .into_potree_builder()
        .name(name)
        .target_scale(scale_arr)
        .max_points_per_node(args.max_points_per_node)
        .max_depth(args.max_depth);
    if let Some(seed) = args.seed {
        builder = builder.seed(seed);
    }
    if let Some(projection) = args.projection {
        builder = builder.projection(projection);
    }

    let buffers = builder.build()?;

    let octree_path = output.join("octree.bin");
    let hierarchy_path = output.join("hierarchy.bin");
    let metadata_path = output.join("metadata.json");

    std::fs::write(&octree_path, &buffers.octree)?;
    std::fs::write(&hierarchy_path, &buffers.hierarchy)?;
    std::fs::write(&metadata_path, &buffers.metadata_json)?;

    println!("Wrote Potree output to {}", output.display());

    Ok(())
}
