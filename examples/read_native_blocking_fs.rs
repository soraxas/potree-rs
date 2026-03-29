use potree::blocking::asset::fs::BlockingPotreeFsAsset;
use potree::blocking::hierarchy::HierarchySync;
use potree::octree::node::{iter_one_bits, NodeType};
use potree::prelude::*;
use std::collections::VecDeque;

pub fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    tracing_subscriber::fmt::init();

    let path: &str = "assets/heidentor";

    let potree_asset = BlockingPotreeFsAsset::from_path(path);

    tracing::info!("Load pointcloud from local filesystem path {}", path);
    let hierarchy = Hierarchy::new_blocking(potree_asset)?;

    let nodes = hierarchy
        .load_initial_hierarchy()
        .expect("Unable to load initial hierarchy");
    tracing::info!(
        "Successfuly loaded point cloud hierarchy with {} nodes",
        nodes.len()
    );

    // load all initial hierarchy points
    for node in nodes {
        // load points only for nodes (not proxies)
        if matches!(node.node_type, NodeType::Node) {
            match hierarchy.load_points(&node) {
                Ok(points) => {
                    tracing::info!(
                        "Loaded {} points for node {}",
                        points.points.len(),
                        node.name
                    );
                }
                Err(error) => {
                    tracing::error!("Unable to load points for node {}: {:#?}", node.name, error);
                    tracing::info!("Node: {:#?}", node);
                }
            }
        }
    }

    // This will load the entire hierarchy
    let all_nodes = hierarchy
        .load_entire_hierarchy()
        .expect("Unable to load entire hierarchy");

    tracing::info!(
        "Successfuly loaded entire point cloud hierarchy with {} nodes",
        all_nodes.len()
    );

    // Perform some checks
    for (i, node) in all_nodes.iter().enumerate() {
        let node_type: u8 = node.node_type.into();

        if let Some(parent) = node.parent {
            let parent = &all_nodes[parent];
            let parent_name = &node.name[0..node.name.len() - 1];
            if !parent.name.eq(parent_name) {
                panic!("Invalid hierarchy: {:#?}", node);
            }
        }

        tracing::debug!(
            "{}: {} parent = {} child_index = {} children = {:?} children_mask = {:b} node_type: {}",
            i,
            node.name,
            node.parent.unwrap_or_default(),
            node.child_index,
            node.children,
            node.children_mask,
            node_type,
        );
    }

    let mut stack: VecDeque<_> = all_nodes.clone().into();

    let mut index = 0;
    while let Some(node) = stack.pop_front() {
        for child_index in iter_one_bits(node.children_mask) {
            let child_index_absolute = node.children[child_index as usize];
            let child = &all_nodes[child_index_absolute];
            assert_eq!(child.parent, Some(index));
            assert_eq!(child.child_index, child_index);
            let child_name = format!("{}{}", node.name, child_index);
            assert_eq!(child.name, child_name);
        }

        index += 1;
    }

    Ok(())
}
