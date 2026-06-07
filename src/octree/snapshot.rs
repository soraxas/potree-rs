use crate::octree::aabb::Aabb;
use crate::octree::node::{NodeType, OctreeNode};
use crate::octree::NodeId;

#[derive(Clone, Debug, Default)]
pub struct OctreeNodeSnapshot {
    pub parent_id: Option<NodeId>,
    pub id: Option<NodeId>,
    pub index: usize,
    // the child index from the parent's view (valid only for non root)
    pub child_index: u8,
    pub name: String,
    pub bounding_box: Aabb,
    pub spacing: f32,
    pub level: u32,
    pub node_type: NodeType,
    pub num_points: u32,
    pub byte_offset: u64,
    pub byte_size: u64,
    pub hierarchy_byte_offset: u64,
    pub hierarchy_byte_size: u64,
    pub children: [usize; 8],
    pub children_mask: u8,
}

impl From<&OctreeNode> for OctreeNodeSnapshot {
    fn from(node: &OctreeNode) -> Self {
        Self {
            parent_id: node.parent,
            id: node.id,
            // unknown index, defaults to 0
            index: 0,
            child_index: node.child_index,
            name: node.name.clone(),
            bounding_box: node.bounding_box.clone(),
            spacing: node.spacing,
            level: node.level,
            node_type: node.node_type,
            num_points: node.num_points,
            byte_offset: node.byte_offset,
            byte_size: node.byte_size,
            hierarchy_byte_offset: node.hierarchy_byte_offset,
            hierarchy_byte_size: node.hierarchy_byte_size,
            // fill with no children
            children: [0; 8],
            children_mask: 0u8,
        }
    }
}
