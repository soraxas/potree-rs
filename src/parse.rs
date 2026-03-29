use std::io::Cursor;

use binrw::{binrw, BinReaderExt};
use thiserror::Error;

use crate::octree::{
    aabb::create_child_aabb,
    node::{NodeType, OctreeNode},
};

#[derive(Debug, Error)]
pub enum ParseHierarchyError {
    // #[error("Hierarchy is already loaded")]
    // AlreadyLoaded,

    // #[error("Invalid json: {0}")]
    // JsonError(#[from] serde_json::error::Error),

    // #[error("IO Error")]
    // Io(#[from] std::io::Error),

    // #[error("Asset error: {0}")]
    // Asset(#[from] PotreeAssetError),
    #[error("Invalid binary data")]
    InvalidBinaryData(#[from] binrw::error::Error),
    // #[error("There is nothing to load because node is not a proxy")]
    // NothingToLoad,

    // #[error("The referenced node does not exist")]
    // NodeNotFound,
}

#[binrw]
#[derive(Clone, Debug)]
#[br(little)]
pub struct HierarchyNodeEntry {
    pub r#type: u8,
    pub child_mask: u8,
    pub num_points: u32,
    pub byte_offset: u64,
    pub byte_size: u64,
}

pub fn parse_flat_hierarchy(
    proxy_node: &OctreeNode,
    buf: &[u8],
) -> Result<Vec<OctreeNode>, ParseHierarchyError> {
    const BYTES_PER_NODE: usize = 22;
    let mut cursor = Cursor::new(buf);
    let num_nodes = buf.len() / BYTES_PER_NODE;

    if num_nodes == 0 {
        return Ok(Vec::new());
    }

    // allocate nodes
    let mut nodes = vec![OctreeNode::default(); num_nodes];

    // set first node
    nodes[0] = proxy_node.clone();
    // remove the parent because it becomes the root
    nodes[0].parent = None;

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

        for child_index in 0_u8..8 {
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

            children[child_index as usize] = node_pos;

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
