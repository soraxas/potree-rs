use std::cmp::max;
use potree::prelude::*;

#[tokio::main(flavor = "current_thread")]
pub async fn main() {
    tracing_subscriber::fmt::init();

    let url: &str = "file://assets/heidentor";
    // let url: &str = "file:///home/romain/Documents/Potree/Liban";

    tracing::info!("Load pointcloud from local filesystem");
    let mut point_cloud =
        Hierarchy::from_url(url, ResourceLoader::new())
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

    tracing::info!("Loaded {} points with occupancy {}", points.points.len(), points.density);

    // let mut max_density = 0;
    // for node in full_snapshot {
    //     let points = point_cloud
    //         .load_points(node.id.unwrap())
    //         .await
    //         .expect("Unable to load points");
    //     if points.density > max_density {
    //         max_density = points.density;
    //     }
    // }
    //
    // tracing::info!("Max density: {}", max_density);

}
