use potree::prelude::*;

#[tokio::main(flavor = "current_thread")]
pub async fn main() {
    tracing_subscriber::fmt::init();

    let url: &str = "file://assets/heidentor";

    tracing::info!("Load pointcloud from local filesystem");
    let mut point_cloud = PointCloud::from_url(url, ResourceLoader::new())
        .await
        .expect("Unable to load point cloud");

    let snapshot = point_cloud.hierarchy_snapshot();
    tracing::info!(
        "Successfuly loaded point cloud hierarchy with {} nodes",
        snapshot.len()
    );

    point_cloud
        .load_entire_hierarchy()
        .await
        .expect("Unable to load entire hierarchy");

    let full_snapshot = point_cloud.hierarchy_snapshot();
    tracing::info!(
        "Successfuly loaded entire point cloud hierarchy with {} nodes.",
        full_snapshot.len()
    );

    let points = point_cloud
        .load_points(point_cloud.octree().root_id())
        .await
        .expect("Unable to load points");

    tracing::info!(
        "Loaded {} points with occupancy {}",
        points.points.len(),
        points.density
    );
}
