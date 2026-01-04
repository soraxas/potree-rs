use potree::prelude::*;

#[tokio::main(flavor = "current_thread")]
pub async fn main() {
    tracing_subscriber::fmt::init();

    let url: &str = "https://potree.org/pointclouds/heidentor/";

    tracing::info!("Load pointcloud from url {}", url);
    let hierarchy = Hierarchy::from_url(url, ResourceLoader::new())
        .await
        .expect("Unable to load point cloud");

    let nodes = hierarchy
        .load_initial_hierarchy()
        .await
        .expect("Unable to load initial hierarchy");
    tracing::info!(
        "Successfuly loaded point cloud hierarchy with {} nodes",
        nodes.len()
    );

    let points = hierarchy
        .load_points(&nodes[0])
        .await
        .expect("Unable to load points");
    tracing::info!(
        "Successfuly loaded {} points for the root node",
        points.points.len()
    );
}
