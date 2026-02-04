use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use byteorder::{LittleEndian, WriteBytesExt};
use serde_json::json;
use thiserror::Error;

pub mod ply_loader;

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid input: {0}")]
    InvalidInput(String),
}

pub const HIERARCHY_BYTES_PER_NODE: usize = 22;

#[derive(Debug, Clone)]
pub struct PotreeBuffers {
    pub metadata_json: Vec<u8>,
    pub hierarchy: Vec<u8>,
    pub octree: Vec<u8>,
}

pub fn compute_scale_offset(
    min: [f64; 3],
    max: [f64; 3],
    target_scale: [f64; 3],
) -> ([f64; 3], [f64; 3]) {
    let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];

    let min_scale = [
        size[0] / 2f64.powi(30),
        size[1] / 2f64.powi(30),
        size[2] / 2f64.powi(30),
    ];

    let scale = [
        target_scale[0].max(min_scale[0]),
        target_scale[1].max(min_scale[1]),
        target_scale[2].max(min_scale[2]),
    ];

    let offset = min;

    (scale, offset)
}

pub fn estimate_spacing(min: [f64; 3], max: [f64; 3], points: u64) -> f64 {
    if points == 0 {
        return 0.0;
    }

    let size = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let volume = size[0].max(0.0) * size[1].max(0.0) * size[2].max(0.0);
    if volume <= 0.0 {
        return 0.0;
    }

    (volume / points as f64).cbrt()
}

fn write_octree_bin_positions_impl<W: Write>(
    writer: &mut W,
    positions: &[[f64; 3]],
    colors: Option<&[[u16; 3]]>,
    scale: [f64; 3],
    offset: [f64; 3],
) -> Result<u64, ConvertError> {
    let write_color = colors.is_some();

    for (idx, p) in positions.iter().enumerate() {
        let ix = quantize_i32(p[0], scale[0], offset[0]);
        let iy = quantize_i32(p[1], scale[1], offset[1]);
        let iz = quantize_i32(p[2], scale[2], offset[2]);

        writer.write_i32::<LittleEndian>(ix)?;
        writer.write_i32::<LittleEndian>(iy)?;
        writer.write_i32::<LittleEndian>(iz)?;

        if write_color {
            let colors = colors
                .ok_or_else(|| ConvertError::InvalidInput("missing colors array".to_string()))?;
            let [r, g, b] = colors
                .get(idx)
                .ok_or_else(|| ConvertError::InvalidInput("color count mismatch".to_string()))?;
            writer.write_u16::<LittleEndian>(*r)?;
            writer.write_u16::<LittleEndian>(*g)?;
            writer.write_u16::<LittleEndian>(*b)?;
        }
    }

    let bytes_per_point = if write_color { 18 } else { 12 };
    Ok((positions.len() * bytes_per_point) as u64)
}

pub fn write_octree_bin_positions(
    path: &Path,
    positions: &[[f64; 3]],
    colors: Option<&[[u16; 3]]>,
    scale: [f64; 3],
    offset: [f64; 3],
) -> Result<u64, ConvertError> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    let byte_size = write_octree_bin_positions_impl(&mut writer, positions, colors, scale, offset)?;
    writer.flush()?;

    Ok(byte_size)
}

pub fn build_octree_bin_positions(
    positions: &[[f64; 3]],
    colors: Option<&[[u16; 3]]>,
    scale: [f64; 3],
    offset: [f64; 3],
) -> Result<(Vec<u8>, u64), ConvertError> {
    let mut buffer = Vec::new();
    let byte_size = write_octree_bin_positions_impl(&mut buffer, positions, colors, scale, offset)?;
    Ok((buffer, byte_size))
}

pub fn write_single_root_hierarchy(
    path: &Path,
    num_points: u32,
    byte_size: u64,
) -> Result<(), ConvertError> {
    let data = build_single_root_hierarchy(num_points, byte_size)?;
    let mut file = File::create(path)?;
    file.write_all(&data)?;

    Ok(())
}

pub fn build_single_root_hierarchy(
    num_points: u32,
    byte_size: u64,
) -> Result<Vec<u8>, ConvertError> {
    let mut data = Vec::with_capacity(HIERARCHY_BYTES_PER_NODE);

    let node_type: u8 = 1; // LEAF
    let child_mask: u8 = 0;
    let byte_offset: u64 = 0;

    data.write_u8(node_type)?;
    data.write_u8(child_mask)?;
    data.write_u32::<LittleEndian>(num_points)?;
    data.write_u64::<LittleEndian>(byte_offset)?;
    data.write_u64::<LittleEndian>(byte_size)?;

    Ok(data)
}

pub fn write_metadata_json(
    path: &Path,
    name: &str,
    projection: &str,
    points: u64,
    min: [f64; 3],
    max: [f64; 3],
    scale: [f64; 3],
    offset: [f64; 3],
    spacing: f64,
    encoding: &str,
    has_color: bool,
) -> Result<(), ConvertError> {
    let content = build_metadata_json(
        name, projection, points, min, max, scale, offset, spacing, encoding, has_color,
    )?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = File::create(path)?;
    file.write_all(&content)?;

    Ok(())
}

pub fn build_metadata_json(
    name: &str,
    projection: &str,
    points: u64,
    min: [f64; 3],
    max: [f64; 3],
    scale: [f64; 3],
    offset: [f64; 3],
    spacing: f64,
    encoding: &str,
    has_color: bool,
) -> Result<Vec<u8>, ConvertError> {
    let mut attributes = vec![json!({
        "name": "position",
        "description": "",
        "size": 12,
        "numElements": 3,
        "elementSize": 4,
        "type": "int32",
        "min": [min[0], min[1], min[2]],
        "max": [max[0], max[1], max[2]],
        "scale": [scale[0], scale[1], scale[2]],
        "offset": [offset[0], offset[1], offset[2]],
    })];

    if has_color {
        attributes.push(json!({
            "name": "rgb",
            "description": "",
            "size": 6,
            "numElements": 3,
            "elementSize": 2,
            "type": "uint16",
            "min": [0, 0, 0],
            "max": [65535, 65535, 65535]
        }));
    }

    let metadata = json!({
        "version": "2.0",
        "name": name,
        "description": "",
        "points": points,
        "projection": projection,
        "hierarchy": {
            "firstChunkSize": HIERARCHY_BYTES_PER_NODE,
            "stepSize": 5,
            "depth": 0
        },
        "offset": [offset[0], offset[1], offset[2]],
        "scale": [scale[0], scale[1], scale[2]],
        "spacing": spacing,
        "boundingBox": {
            "min": [min[0], min[1], min[2]],
            "max": [max[0], max[1], max[2]]
        },
        "encoding": encoding,
        "attributes": attributes
    });

    let content = serde_json::to_vec_pretty(&metadata)
        .map_err(|err| ConvertError::InvalidInput(err.to_string()))?;
    Ok(content)
}

fn quantize_i32(value: f64, scale: f64, offset: f64) -> i32 {
    if scale == 0.0 {
        return 0;
    }
    let scaled = (value - offset) / scale;
    let rounded = scaled.round();

    if rounded > i32::MAX as f64 {
        i32::MAX
    } else if rounded < i32::MIN as f64 {
        i32::MIN
    } else {
        rounded as i32
    }
}

pub fn build_potree_buffers(
    name: &str,
    projection: &str,
    positions: &[[f64; 3]],
    colors: Option<&[[u16; 3]]>,
    target_scale: [f64; 3],
    encoding: &str,
) -> Result<PotreeBuffers, ConvertError> {
    if positions.is_empty() {
        return Err(ConvertError::InvalidInput("no positions provided".to_string()));
    }

    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];

    for p in positions {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }

    let (scale, offset) = compute_scale_offset(min, max, target_scale);
    let points = positions.len() as u64;
    let spacing = estimate_spacing(min, max, points);

    let (octree, byte_size) =
        build_octree_bin_positions(positions, colors, scale, offset)?;
    let hierarchy = build_single_root_hierarchy(points as u32, byte_size)?;
    let metadata_json = build_metadata_json(
        name,
        projection,
        points,
        min,
        max,
        scale,
        offset,
        spacing,
        encoding,
        colors.is_some(),
    )?;

    Ok(PotreeBuffers {
        metadata_json,
        hierarchy,
        octree,
    })
}
