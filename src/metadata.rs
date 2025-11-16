use crate::morton::{read_morton_128, read_morton_64};
use crate::octree::aabb::Aabb;
use crate::octree::node::OctreeNode;
use crate::point::PointData;
use crate::resource::{ResourceError, ResourceLoader};
use byteorder::{ByteOrder, LittleEndian};
use glam::{DVec3, U8Vec3, UVec3};
use serde::Deserialize;
use std::io::{Cursor, Read};
use std::ops::Sub;
use thiserror::Error;

const GRID_SIZE: f64 = 32.0;
const GRID_SIZE_UINT: u32 = GRID_SIZE as u32;
const GRID_SIZE_SPLAT: UVec3 = UVec3::splat(GRID_SIZE as u32 - 1);

pub struct Points {
    pub points: Vec<PointData>,
    pub density: u32,
}

#[derive(Error, Debug)]
pub enum LoadPointsError {
    #[error("Node does not exists")]
    NodeNotFound,

    #[error("Resource error: {0}")]
    Resource(#[from] ResourceError),

    #[error("Encoding not implemented: {0}")]
    EncodingUnimplemented(String),

    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub version: String,
    pub name: String,
    pub description: String,
    pub points: u64,
    pub projection: String,
    pub hierarchy: HierarchyMetadata,
    pub offset: [f64; 3],
    pub scale: [f64; 3],
    pub spacing: f64,
    pub bounding_box: BoundingBox,
    pub encoding: String,
    pub attributes: Vec<AttributeMetadata>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyMetadata {
    pub first_chunk_size: u64,
    pub step_size: u16,
    pub depth: u16,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BoundingBox {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Deserialize, Clone, Debug)]
pub enum AttributeType {
    #[serde(rename = "int8")]
    Int8,
    #[serde(rename = "int16")]
    Int16,
    #[serde(rename = "int32")]
    Int32,
    #[serde(rename = "int64")]
    Int64,
    #[serde(rename = "uint8")]
    UInt8,
    #[serde(rename = "uint16")]
    UInt16,
    #[serde(rename = "uint32")]
    UInt32,
    #[serde(rename = "uint64")]
    UInt64,
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "double")]
    Double,
    #[serde(rename = "undefined")]
    Undefined,
}
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AttributeMetadata {
    pub name: String,
    pub description: String,
    /// usually contains total size of a point attribute (`num_elements * element_size`)
    pub size: u16,
    /// number of elements in the attribute
    pub num_elements: u16,
    /// contains a single element size
    pub element_size: u16,
    pub r#type: AttributeType,
    pub min: Vec<f32>,
    pub max: Vec<f32>,
}

impl Metadata {
    pub(crate) fn create_root_node(&self) -> OctreeNode {
        OctreeNode {
            name: "r".to_string(),
            bounding_box: self.bounding_box.clone().into(),
            spacing: self.spacing,
            node_type: 2,
            hierarchy_byte_size: self.hierarchy.first_chunk_size,
            ..Default::default()
        }
    }

    pub async fn load_points_for_node(
        &self,
        node: &OctreeNode,
        octree_url: &str,
        resource_loader: &ResourceLoader,
    ) -> Result<Points, LoadPointsError> {
        let buffer = resource_loader
            .get_range(octree_url, node.byte_offset, node.byte_size as usize, None)
            .await?;

        let points = match self.encoding.as_str() {
            "BROTLI" => self.parse_points_brotli(node, &buffer)?,
            "DEFAULT" => self.parse_points_default(node, &buffer)?,
            _ => {
                return Err(LoadPointsError::EncodingUnimplemented(
                    self.encoding.clone(),
                ));
            }
        };

        Ok(points)
    }

    fn parse_points_default(
        &self,
        node: &OctreeNode,
        buffer: &[u8],
    ) -> Result<Points, LoadPointsError> {
        let mut points = vec![PointData::default(); node.num_points as usize];

        let size = node.bounding_box.max.sub(node.bounding_box.min);
        let mut grid = vec![0_u32; (GRID_SIZE_UINT * GRID_SIZE_UINT * GRID_SIZE_UINT) as usize];
        let mut num_occupied_cells = 0;

        // compute bytes per point
        let mut bytes_per_point = 0;
        for point_attribute in &self.attributes {
            bytes_per_point += point_attribute.size as u32;
        }

        let mut attribute_offset: usize = 0;

        for point_attribute in &self.attributes {
            let attribute_size = point_attribute.size as usize;
            let element_size = point_attribute.element_size as usize;

            match point_attribute.name.as_str() {
                "POSITION_CARTESIAN" | "position" => {
                    let scale = &self.scale;
                    let offset = &self.offset;

                    for j in 0..node.num_points {
                        let point_offset = (j * bytes_per_point) as usize;
                        let bytes = &buffer[(point_offset + attribute_offset)
                            ..(point_offset + attribute_offset + attribute_size)];

                        let x = LittleEndian::read_u32(&bytes[0..element_size]);
                        let y = LittleEndian::read_u32(&bytes[element_size..2 * element_size]);
                        let z = LittleEndian::read_u32(&bytes[2 * element_size..3 * element_size]);

                        let position = DVec3::new(
                            x as f64 * scale[0] + offset[0] - node.bounding_box.min.x,
                            y as f64 * scale[1] + offset[1] - node.bounding_box.min.y,
                            z as f64 * scale[2] + offset[2] - node.bounding_box.min.z,
                        );

                        let index = to_index(&position, &size);
                        grid[index] += 1;
                        if grid[index] == 1 {
                            num_occupied_cells += 1;
                        }

                        points[j as usize].position = position + node.bounding_box.min;
                    }
                }
                "RGBA" | "rgba" | "RGB" | "rgb" => {
                    for j in 0..node.num_points {
                        let point_offset = (j * bytes_per_point) as usize;
                        let bytes = &buffer[(point_offset + attribute_offset)
                            ..(point_offset + attribute_offset + attribute_size)];

                        let r = LittleEndian::read_u16(&bytes[0..element_size]);
                        let g = LittleEndian::read_u16(&bytes[element_size..2 * element_size]);
                        let b = LittleEndian::read_u16(&bytes[2 * element_size..3 * element_size]);

                        points[j as usize].color = U8Vec3::new(
                            if r > 255 { r / 256 } else { r } as u8,
                            if g > 255 { g / 256 } else { g } as u8,
                            if b > 255 { b / 256 } else { b } as u8,
                        );
                    }
                }
                _ => {}
            }

            attribute_offset += attribute_size;
        }

        // println!("Final offset: {}, size: {}", byte_offset, size);

        Ok(Points {
            points,
            density: node.num_points / num_occupied_cells,
        })
    }

    fn parse_points_brotli(
        &self,
        node: &OctreeNode,
        buffer: &[u8],
    ) -> Result<Points, LoadPointsError> {
        let mut cursor = Cursor::new(buffer);
        let mut input = brotli_decompressor::Decompressor::new(&mut cursor, 4096);
        let mut decompressed_buffer = Vec::new();
        input.read_to_end(&mut decompressed_buffer)?;

        let mut byte_offset: usize = 0;

        let mut points = vec![PointData::default(); node.num_points as usize];

        let size = node.bounding_box.max.sub(node.bounding_box.min);
        let mut grid = vec![0_u32; (GRID_SIZE_UINT * GRID_SIZE_UINT * GRID_SIZE_UINT) as usize];
        let mut num_occupied_cells = 0;

        for point_attribute in &self.attributes {
            match point_attribute.name.as_str() {
                "POSITION_CARTESIAN" | "position" => {
                    let scale = &self.scale;
                    let offset = &self.offset;

                    for j in 0..node.num_points {
                        let bytes = &decompressed_buffer[byte_offset..byte_offset + 16];
                        let (x, y, z) = read_morton_128(bytes);

                        let position = DVec3::new(
                            x as f64 * scale[0] + offset[0] - node.bounding_box.min.x,
                            y as f64 * scale[1] + offset[1] - node.bounding_box.min.y,
                            z as f64 * scale[2] + offset[2] - node.bounding_box.min.z,
                        );

                        let index = to_index(&position, &size);
                        grid[index] += 1;
                        if grid[index] == 1 {
                            num_occupied_cells += 1;
                        }

                        points[j as usize].position = position + node.bounding_box.min;

                        byte_offset += 16;
                    }
                }
                "RGBA" | "rgba" | "RGB" | "rgb" => {
                    for j in 0..node.num_points {
                        let bytes = &decompressed_buffer[byte_offset..byte_offset + 8];
                        let (r, g, b) = read_morton_64(bytes);

                        points[j as usize].color = U8Vec3::new(
                            if r > 255 { r / 256 } else { r } as u8,
                            if g > 255 { g / 256 } else { g } as u8,
                            if b > 255 { b / 256 } else { b } as u8,
                        );

                        byte_offset += 8;
                    }
                }
                _ => {
                    for j in 0..node.num_points {
                        let bytes = &decompressed_buffer
                            [byte_offset..byte_offset + point_attribute.size as usize];

                        byte_offset += point_attribute.size as usize;
                    }
                }
            }
        }

        // println!("Final offset: {}, size: {}", byte_offset, size);

        Ok(Points {
            points,
            density: node.num_points / num_occupied_cells,
        })
    }
}

fn to_index(position: &DVec3, size: &DVec3) -> usize {
    let index = (GRID_SIZE * position / size)
        .as_uvec3()
        .min(GRID_SIZE_SPLAT);
    (index.x + GRID_SIZE_UINT * index.y + GRID_SIZE_UINT * GRID_SIZE_UINT * index.z) as usize
}

impl Into<Aabb> for BoundingBox {
    fn into(self) -> Aabb {
        Aabb {
            min: self.min.into(),
            max: self.max.into(),
        }
    }
}
