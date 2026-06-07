use potree::{
    blocking::{asset::http::BlockingPotreeHttpAsset, hierarchy::HierarchySync},
    prelude::*,
};

pub fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    tracing_subscriber::fmt::init();

    let url: &str = "https://potree.org/pointclouds/heidentor/";

    let asset = BlockingPotreeHttpAsset::from_url(url);

    tracing::info!("Load pointcloud from url {}", url);
    let hierarchy = Hierarchy::new_blocking(asset).expect("Unable to load point cloud");

    let nodes = hierarchy
        .load_initial_hierarchy()
        .expect("Unable to load initial hierarchy");
    tracing::info!(
        "Successfuly loaded point cloud hierarchy with {} nodes",
        nodes.len()
    );

    let points = hierarchy
        .load_points(&nodes[0])
        .expect("Unable to load points");
    tracing::info!(
        "Successfuly loaded {} points for the root node",
        points.buffer.count
    );

    Ok(())
}
