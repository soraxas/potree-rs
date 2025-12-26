use crate::octree::NodeId;
use super::aabb::Aabb;

#[derive(Clone, Debug, Default, Copy, Eq, PartialEq)]
pub enum NodeType {
    /// a node that has children
    #[default]
    Node,
    /// a node that has no children
    Leaf,
    /// a node not yet loaded
    Proxy,
    /// unsupported node types
    Other(u8),
}

impl NodeType {
    pub fn has_points(&self) -> bool {
        match self {
            NodeType::Node | NodeType::Leaf => true,
            _ => false
        }
    }
}

impl From<u8> for NodeType {
    fn from(value: u8) -> Self {
        match value {
            0 => NodeType::Node,
            1 => NodeType::Leaf,
            2 => NodeType::Proxy,
            _ => NodeType::Other(value),
        }
    }
}

impl Into<u8> for NodeType {
    fn into(self) -> u8 {
        match self {
            NodeType::Node => 0,
            NodeType::Leaf => 1,
            NodeType::Proxy => 2,
            NodeType::Other(other) => other
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct OctreeNode {
    pub name: String,
    pub child_index: u8,
    pub bounding_box: Aabb,
    pub spacing: f64,
    pub level: u32,
    pub node_type: NodeType,
    pub num_points: u32,
    pub byte_offset: u64,
    pub byte_size: u64,
    pub hierarchy_byte_offset: u64,
    pub hierarchy_byte_size: u64,

    // The node's id if known. None means not yet stored.
    pub id: Option<NodeId>,

    // The node's parent id. None means it's the root node.
    pub parent: Option<NodeId>,

    // Preallocated children array
    pub children: [NodeId; 8],
    // children mask: 1 is occupied, 0 is vacant
    pub children_mask: u8,
}

#[derive(Clone, Debug, Default)]
pub struct FlatOctreeNode {
    pub name: String,
    pub child_index: u8,
    pub bounding_box: Aabb,
    pub spacing: f64,
    pub level: u32,
    pub node_type: NodeType,
    pub num_points: u32,
    pub byte_offset: u64,
    pub byte_size: u64,
    pub hierarchy_byte_offset: u64,
    pub hierarchy_byte_size: u64,

    // The node's id if known. None means not yet stored.
    pub id: Option<NodeId>,

    // The node's parent id. None means it's the root node.
    pub parent: Option<usize>,

    // Preallocated children array
    pub children: [usize; 8],
    // children mask: 1 is occupied, 0 is vacant
    pub children_mask: u8,
}

pub trait U8BitExt {
    fn iter_zero_bits(self) -> impl Iterator<Item = usize>;

    fn iter_one_bits(self) -> impl Iterator<Item = usize>;
}

impl U8BitExt for u8 {
    fn iter_zero_bits(self) -> impl Iterator<Item = usize> {
        (0..8).filter(move |&i| (self & (1 << i)) == 0)
    }

    fn iter_one_bits(self) -> impl Iterator<Item = usize> {
        (0..8).filter(move |&i| (self & (1 << i)) != 0)
    }
}


pub fn iter_zero_bits(mask: u8) -> impl Iterator<Item = usize> {
    (0..8).filter(move |&i| (mask & (1 << i)) == 0)
}

pub fn iter_one_bits(mask: u8) -> impl Iterator<Item = u8> {
    (0_u8..8).filter(move |&i| (mask & (1 << i)) != 0)
}
