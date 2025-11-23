use crate::hierarchy::{HierarchyNodeEntry, LoadPotreePointCloudError};
use crate::metadata::{LoadPointsError, Metadata, Points};
use crate::octree::aabb::create_child_aabb;
use crate::octree::node::{FlatOctreeNode, NodeType, OctreeNode, U8BitExt, iter_one_bits};
use crate::octree::snapshot::OctreeNodeSnapshot;
use crate::octree::{FlatOctree, NodeId};
use crate::point::PointData;
use crate::prelude::ReadHierarchyError;
use crate::resource::{ResourceError, ResourceLoader};
use binrw::BinReaderExt;
use binrw::prelude::*;
use std::collections::VecDeque;
use std::io::{Cursor, Read};
use tracing::warn;

#[derive(Clone, Debug)]
pub struct FlatHierarchy {
    metadata: Metadata,
    hierarchy_url: String,
    octree_url: String,
    octree: FlatOctree<OctreeNode>,
    resource_loader: ResourceLoader,
}

impl FlatHierarchy {
    /// Load a Potree point cloud from a URL.
    /// Relatives urls works only if the provided client supports it.
    /// Metadatas, hierarchy and octree are supposed to be accessible relatively to the provided url:
    ///  - Metadata: `<url>/metadata.json`
    ///  - Hierarchy: `<url>/hierarchy.bin`
    ///  - Octree: `<url>/octree.bin`
    pub async fn from_url(
        url: &str,
        resource_loader: ResourceLoader,
    ) -> Result<FlatHierarchy, LoadPotreePointCloudError> {
        let octree = FlatOctree::new();

        let metadata_url = format!("{}/metadata.json", url).to_string();
        let hierarchy_url = format!("{}/hierarchy.bin", url).to_string();
        let metadata = resource_loader
            .get_json(&metadata_url, None)
            .await
            .map_err(|error| LoadPotreePointCloudError::ResourceError(error))?;

        Ok(Self {
            metadata,
            hierarchy_url,
            octree_url: format!("{}/octree.bin", url).to_string(),
            octree,
            resource_loader,
        })
    }

    pub async fn load_initial_hierarchy(&self) -> Result<Vec<FlatOctreeNode>, ReadHierarchyError> {
        // load root node metadatas
        let root = self.metadata.create_flat_root_node();

        // load its hierarchy
        let nodes = self.load_hierarchy(&root).await?;

        Ok(nodes)
    }

    pub async fn load_hierarchy(
        &self,
        node: &FlatOctreeNode,
    ) -> Result<Vec<FlatOctreeNode>, ReadHierarchyError> {
        if matches!(node.node_type, NodeType::Proxy) {
            let data = self
                .resource_loader
                .get_range(
                    &self.hierarchy_url,
                    node.hierarchy_byte_offset,
                    node.hierarchy_byte_size as usize,
                    None,
                )
                .await?;

            Ok(parse_flat_hierarchy(node, &data)?)
        } else {
            // this node is not a proxy, so its hierarchy can't be loaded
            Err(ReadHierarchyError::AlreadyLoaded)
        }
    }

    pub async fn load_entire_hierarchy(&self) -> Result<Vec<FlatOctreeNode>, ReadHierarchyError> {
        let root = self.metadata.create_flat_root_node();

        Ok(self.load_entire_hierarchy_from_proxy(root).await?)
    }

    pub async fn load_entire_hierarchy_from_proxy(
        &self,
        node: FlatOctreeNode,
    ) -> Result<Vec<FlatOctreeNode>, ReadHierarchyError> {
        if !matches!(node.node_type, NodeType::Proxy) {
            // node is not a proxy
            return Err(ReadHierarchyError::NothingToLoad);
        }

        // initialize a stack
        let mut stack = VecDeque::new();
        stack.push_back(node);

        // initialize the output nodes
        let mut nodes: Vec<FlatOctreeNode> = Vec::new();

        // for each node in stack
        while let Some(mut node) = stack.pop_front() {
            let next_index = nodes.len();
            let stack_offset = stack.len();

            let node = match node.node_type {
                // if it's proxy, load it and its hierarchy, and all new nodes to the stack
                NodeType::Proxy => {
                    let nodes = self.load_hierarchy(&node).await?;

                    // assign correct parent index
                    let Some(node) = ({
                        let mut first_node = None;
                        for (i, mut node) in nodes.into_iter().enumerate() {
                            if i == 0 {
                                first_node = Some(node);
                            } else {
                                // update node's parent index
                                match node.parent {
                                    Some(0) => {
                                        node.parent = Some(next_index);
                                    }
                                    Some(parent @ 1..) => {
                                        node.parent = Some(parent + next_index + stack_offset);
                                    }
                                    None => {}
                                }
                                stack.push_back(node);
                            }
                        }
                        first_node
                    }) else {
                        warn!("A proxy node has missing hierarchy, should not happen.");
                        continue;
                    };
                    node
                }
                // else, just process this node
                _ => node,
            };

            // update child index in parent's children
            if let Some(parent) = node.parent {
                nodes[parent].children[node.child_index] = next_index;
            }

            // add the node to the list of nodes
            nodes.push(node);
        }

        Ok(nodes)
    }

    // Functions to load points
    pub async fn load_points(&self, node: FlatOctreeNode) -> Result<Points, LoadPointsError> {
        let buffer = self
            .resource_loader
            .get_range(
                &self.octree_url,
                node.byte_offset,
                node.byte_size as usize,
                None,
            )
            .await?;

        let points = self
            .metadata
            .load_points(node.num_points, &node.bounding_box, &buffer)?;

        Ok(points)
    }

    // Functions to access the octree
    pub fn octree(&self) -> &FlatOctree<OctreeNode> {
        &self.octree
    }
}

pub fn parse_flat_hierarchy(
    proxy_node: &FlatOctreeNode,
    buf: &[u8],
) -> Result<Vec<FlatOctreeNode>, ReadHierarchyError> {
    const BYTES_PER_NODE: usize = 22;
    let mut cursor = Cursor::new(buf);
    let num_nodes = buf.len() / BYTES_PER_NODE;

    // allocate nodes
    let mut nodes = vec![FlatOctreeNode::default(); num_nodes];

    // set first node
    nodes[0] = proxy_node.clone();

    // position of the next node to write in
    let mut node_pos = 1;

    // the first node is always the root of the (sub-)hierarchy we are loading
    for i in 0..num_nodes {
        let current = &mut nodes[i];

        let header: HierarchyNodeEntry = cursor.read_le()?;

        if matches!(current.node_type, NodeType::Proxy) {
            current.byte_offset = header.byte_offset;
            current.byte_size = header.byte_size;
            current.num_points = header.num_points;
        } else if header.r#type == 2 {
            current.hierarchy_byte_offset = header.byte_offset;
            current.hierarchy_byte_size = header.byte_size;
            current.num_points = header.num_points;
        } else {
            current.byte_offset = header.byte_offset;
            current.byte_size = header.byte_size;
            current.num_points = header.num_points;
        }

        if current.byte_size == 0 {
            // workaround for issue https://github.com/potree/potree/issues/1125
            // some inner nodes erroneously report >0 points even though have 0 points
            // however, they still report a ByteSize of 0, so based on that we now set node.NumPoints to 0
            current.num_points = 0;
        }

        current.node_type = header.r#type.into();

        if matches!(current.node_type, NodeType::Proxy) {
            // the children are not yet known, no need to process them
            continue;
        }

        let mut children: [usize; 8] = [0_usize; 8];

        // clone/copy just what we need
        let (current_name, current_bounding_box, current_spacing, current_level) = (
            current.name.clone(),
            current.bounding_box.clone(),
            current.spacing,
            current.level,
        );

        for child_index in 0..8 {
            let child_exists = ((1 << child_index) & header.child_mask) != 0;
            if !child_exists {
                continue;
            }

            // get mutable access to the pre-allocated child
            let child = &mut nodes[node_pos];
            child
                .name
                .push_str(&format!("{}{}", current_name, child_index));
            child.bounding_box = create_child_aabb(&current_bounding_box, child_index);
            child.spacing = current_spacing / 2.0;
            child.level = current_level + 1;
            child.parent = Some(i);
            child.child_index = child_index;

            children[child_index] = node_pos;

            // increment node_pos for the next child
            node_pos += 1;
        }

        // finally, append the children to the parent
        let current = &mut nodes[i];
        current.children = children;
        current.children_mask = header.child_mask;
    }

    Ok(nodes)
}
