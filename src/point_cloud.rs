use crate::hierarchy::Hierarchy;
use crate::metadata::{LoadPointsError, Points};
use crate::octree::node::{iter_one_bits, NodeType, OctreeNode};
use crate::octree::snapshot::OctreeNodeSnapshot;
use crate::octree::{NodeId, Octree};
use crate::resource::{ResourceError, ResourceLoader};
use binrw::prelude::*;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LoadPotreePointCloudError {
    #[error("Error loading metadatas: {0}")]
    LoadMetadataError(ResourceError),

    #[error("Error loading hierarchy: {0}")]
    ReadHierarchyError(#[from] ReadHierarchyError),

    #[error("Error loading resource: {0}")]
    ResourceError(#[from] ResourceError),
}

#[derive(Error, Debug)]
pub enum ReadHierarchyError {
    #[error("Hierarchy is already loaded")]
    AlreadyLoaded,

    #[error("Invalid json: {0}")]
    JsonError(#[from] serde_json::error::Error),

    #[error("IO Error")]
    Io(#[from] std::io::Error),

    #[error("Resource error: {0}")]
    Resource(#[from] ResourceError),

    #[error("Invalid binary data")]
    InvalidBinaryData(#[from] binrw::error::Error),

    #[error("There is nothing to load because node is not a proxy")]
    NothingToLoad,

    #[error("The referenced node does not exist")]
    NodeNotFound,
}

#[derive(Clone, Debug)]
pub struct PointCloud {
    hierarchy: Hierarchy,
    octree: Octree<OctreeNode>,
}

impl PointCloud {
    /// Load a Potree point cloud from a URL.
    /// Relatives urls works only if the provided client supports it.
    /// Metadatas, hierarchy and octree are supposed to be accessible relatively to the provided url:
    ///  - Metadata: `<url>/metadata.json`
    ///  - Hierarchy: `<url>/hierarchy.bin`
    ///  - Octree: `<url>/octree.bin`
    pub async fn from_url(
        url: &str,
        resource_loader: ResourceLoader,
    ) -> Result<PointCloud, LoadPotreePointCloudError> {
        let hierarchy = Hierarchy::from_url(url, resource_loader).await?;
        let octree = Octree::new();

        let mut this = Self { hierarchy, octree };

        this.load_initial_hierarchy().await?;

        Ok(this)
    }

    async fn load_initial_hierarchy(&mut self) -> Result<(), ReadHierarchyError> {
        // load root node metadatas
        let initial_hierarchy = self.hierarchy.load_initial_hierarchy().await?;

        // this vec will store each inserted nodes
        let mut parents = Vec::with_capacity(initial_hierarchy.len());
        for mut node in initial_hierarchy {
            let mut parent_id = None;
            if let Some(parent) = node.parent {
                parent_id = Some(parents[parent]);
            }

            // insert the new node
            let vacant_entry = self.octree.vacant_entry();
            let current_id = vacant_entry.key();
            parents.push(current_id);

            let child_index = node.child_index;

            node.parent = parent_id;
            node.children = [0; 8];
            vacant_entry.insert(node);

            // update parent children / children_mask
            if let Some(parent_id) = &parent_id {
                // infallible because it was inserted upper
                let parent = self.octree.node_mut(*parent_id).unwrap();

                // add to children array and update mask
                parent.children[child_index as usize] = current_id;
                parent.children_mask |= 1u8 << child_index;
            }
        }

        Ok(())
    }

    pub async fn load_hierarchy(&mut self, node_id: NodeId) -> Result<(), ReadHierarchyError> {
        // get the root node
        let node = self
            .octree
            .node(node_id)
            .ok_or_else(|| ReadHierarchyError::NodeNotFound)?;

        if matches!(node.node_type, NodeType::Proxy) {
            let nodes = self.hierarchy.load_hierarchy(node).await?;

            // this vec will store each inserted nodes
            let mut parents = Vec::with_capacity(nodes.len());

            // insert each new hierarchy node in the asset
            for mut node in nodes {
                if let Some(parent) = node.parent {
                    // get the corresponding parent's node id from the indexed vec
                    let parent_id = Some(parents[parent]);

                    // insert the new node
                    let vacant_entry = self.octree.vacant_entry();
                    let current_id = vacant_entry.key();
                    parents.push(current_id);

                    let child_index = node.child_index;

                    node.parent = parent_id;
                    node.children = [0; 8];
                    vacant_entry.insert(node);

                    // update parent children / children_mask
                    if let Some(parent_id) = &parent_id {
                        // infallible because it was inserted upper
                        let parent = self.octree.node_mut(*parent_id).unwrap();

                        // add to children array and update mask
                        parent.children[child_index as usize] = current_id;
                        parent.children_mask |= 1u8 << child_index;
                    }
                } else {
                    parents.push(node_id);
                    // this is the first node, it exists already, just update it
                    let existing_node = self
                        .octree
                        .node_mut(node_id)
                        .ok_or_else(|| ReadHierarchyError::NodeNotFound)?;
                    existing_node.node_type = node.node_type;
                    existing_node.num_points = node.num_points;
                    existing_node.byte_offset = node.byte_offset;
                    existing_node.byte_size = node.byte_size;
                    existing_node.hierarchy_byte_offset = node.hierarchy_byte_offset;
                    existing_node.hierarchy_byte_size = node.hierarchy_byte_size;
                }
            }
        }

        Ok(())
    }

    pub async fn load_entire_hierarchy(&mut self) -> Result<(), ReadHierarchyError> {
        // get the root node
        let root = self
            .octree
            .root()
            .ok_or_else(|| ReadHierarchyError::NodeNotFound)?;
        let children = root.children;

        for i in iter_one_bits(root.children_mask) {
            let child = children[i as usize];
            self.load_entire_hierarchy_recursive(child).await?;
        }

        Ok(())
    }

    pub async fn load_entire_hierarchy_recursive(
        &mut self,
        node_id: NodeId,
    ) -> Result<(), ReadHierarchyError> {
        // load node's hierarchy if needed
        self.load_hierarchy(node_id).await?;

        // get the node
        let node = self
            .octree
            .node(node_id)
            .ok_or_else(|| ReadHierarchyError::NodeNotFound)?;

        let children = node.children;

        for i in iter_one_bits(node.children_mask) {
            let child = children[i as usize];
            Box::pin(self.load_entire_hierarchy_recursive(child)).await?;
        }

        Ok(())
    }

    /// Takes a snapshot of the current loaded hierarchy and return it
    pub fn hierarchy_snapshot(&self) -> Vec<OctreeNodeSnapshot> {
        let Some(root) = self.octree.root() else {
            return Vec::new();
        };
        self.hierarchy_snaphot_from_node(root)
    }

    fn hierarchy_snaphot_from_node(&self, node: &OctreeNode) -> Vec<OctreeNodeSnapshot> {
        let mut stack = vec![(0_usize, node)];
        let mut nodes = Vec::new();

        while let Some((parent_index, node)) = stack.pop() {
            // get the current node future index
            let current_index = nodes.len();

            // process children
            for i in iter_one_bits(node.children_mask) {
                let child = &node.children[i as usize];

                let child = self
                    .octree
                    .node(*child)
                    .expect("missing node in hierarchy, shouldn't happen");
                stack.push((current_index, child));
            }

            // add the current node to the nodes array
            let mut node_snapshot: OctreeNodeSnapshot = node.into();
            node_snapshot.index = current_index;
            nodes.push(node_snapshot);

            // if there is a parent, add it to the children array on an empty space
            if parent_index < current_index {
                let parent_node = &mut nodes[parent_index];
                *parent_node
                    .children
                    .iter_mut()
                    .find(|child| **child == 0)
                    .expect("no empty child space available, there might be a problem") =
                    current_index;
            }
        }

        nodes
    }

    // Functions to load points
    pub async fn load_points(&self, node_id: NodeId) -> Result<Points, LoadPointsError> {
        let node = self
            .octree
            .node(node_id)
            .ok_or(LoadPointsError::NodeNotFound)?;

        self.hierarchy.load_points(node).await
    }

    // Functions to access the octree
    pub fn octree(&self) -> &Octree<OctreeNode> {
        &self.octree
    }
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
