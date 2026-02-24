use std::path::PathBuf;

use clap::Parser;
use potree::convert::streaming::convert_ply_streaming;

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

    /// Output encoding: DEFAULT (raw AoS) or BROTLI (SoA + Brotli compression)
    #[arg(long, default_value = "BROTLI")]
    encoding: String,
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

    convert_ply_streaming(
        &input,
        &output,
        &name,
        &args.projection.unwrap_or_default(),
        scale_arr,
        args.max_points_per_node,
        args.max_depth,
        args.seed,
        &args.encoding,
    )?;

    println!("Wrote Potree output to {}", output.display());

    Ok(())
}
