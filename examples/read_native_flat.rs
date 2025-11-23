use potree::prelude::*;
use std::cmp::max;
use std::collections::VecDeque;
use potree::octree::node::iter_one_bits;

#[tokio::main(flavor = "current_thread")]
pub async fn main() {
    tracing_subscriber::fmt::init();

    // let url: &str = "file://assets/heidentor";
    let url: &str = "file:///home/romain/Documents/Potree/Liban";

    tracing::info!("Load pointcloud from local filesystem");
    let mut point_cloud = FlatHierarchy::from_url(url, ResourceLoader::new())
        .await
        .expect("Unable to load point cloud");

    let nodes = point_cloud
        .load_initial_hierarchy()
        .await
        .expect("Unable to load initial hierarchy");
    tracing::info!(
        "Successfuly loaded point cloud hierarchy with {} nodes",
        nodes.len()
    );

    let all_nodes = point_cloud
        .load_entire_hierarchy()
        .await
        .expect("Unable to load entire hierarchy");

    tracing::info!(
        "Successfuly loaded entire point cloud hierarchy with {} nodes",
        all_nodes.len()
    );

    for (i, node) in all_nodes.iter().enumerate() {
        let node_type: u8 = node.node_type.into();

        if let Some(parent) = node.parent {
            let parent = &all_nodes[parent];
            let parent_name = &node.name[0..node.name.len() - 1];
            if !parent.name.eq(parent_name) {
                panic!("Invalid flat hierarchy: {:#?}", node);
            }
        }

        // println!(
        //     "{}: {} parent = {} child_index = {} children = {:?} children_mask = {:b} node_type: {}",
        //     i,
        //     node.name,
        //     node.parent.unwrap_or_default(),
        //     node.child_index,
        //     node.children,
        //     node.children_mask,
        //     node_type,
        // );
    }

    let mut stack: VecDeque<_> = all_nodes.clone().into();

    let mut index = 0;
    while let Some(node) = stack.pop_front() {
        for child_index in iter_one_bits(node.children_mask) {
            let child_index_absolute = node.children[child_index];
            let child = &all_nodes[child_index_absolute];
            assert_eq!(child.parent, Some(index));
            assert_eq!(child.child_index, child_index);
            let child_name = format!("{}{}", node.name, child_index);
            assert_eq!(child.name, child_name);
        }

        index += 1;
    }

    // let full_snapshot = point_cloud.hierarchy_snapshot();
    // tracing::info!(
    //     "Successfuly loaded entire point cloud hierarchy with {} nodes.",
    //     full_snapshot.len()
    // );
    //
    // let points = point_cloud
    //     .load_points(point_cloud.octree().root_id())
    //     .await
    //     .expect("Unable to load points");
    //
    // tracing::info!("Loaded {} points with occupancy {}", points.points.len(), points.density);

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
