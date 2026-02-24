use crate::resource::buffer::{
    build_metadata_json, compute_scale_offset, estimate_spacing, ConvertError, HIERARCHY_BYTES_PER_NODE,
};
use byteorder::{BigEndian, ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};
use indicatif::{ProgressBar, ProgressStyle};
use rand::{rngs::StdRng, SeedableRng};
use std::collections::{HashSet, VecDeque};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct Node {
    name: String,
    level: u32,
    min: [f64; 3],
    max: [f64; 3],
    children: [Option<usize>; 8],
    child_mask: u8,
    num_points: u32,
    byte_offset: u64,
    byte_size: u64,
    temp_path: Option<PathBuf>,
    sample_data: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
enum PlyFormat {
    Ascii,
    BinaryLittleEndian,
    BinaryBigEndian,
}

#[derive(Debug, Clone, Copy)]
enum PlyScalar {
    Char,
    UChar,
    Short,
    UShort,
    Int,
    UInt,
    Float,
    Double,
}

impl PlyScalar {
    fn byte_size(self) -> usize {
        match self {
            PlyScalar::Char | PlyScalar::UChar => 1,
            PlyScalar::Short | PlyScalar::UShort => 2,
            PlyScalar::Int | PlyScalar::UInt | PlyScalar::Float => 4,
            PlyScalar::Double => 8,
        }
    }
    fn potree_type(self) -> &'static str {
        match self {
            PlyScalar::Char => "int8",
            PlyScalar::UChar => "uint8",
            PlyScalar::Short => "int16",
            PlyScalar::UShort => "uint16",
            PlyScalar::Int => "int32",
            PlyScalar::UInt => "uint32",
            PlyScalar::Float => "float",
            PlyScalar::Double => "double",
        }
    }
    fn value_range(self) -> [f64; 2] {
        match self {
            PlyScalar::Char => [-128.0, 127.0],
            PlyScalar::UChar => [0.0, 255.0],
            PlyScalar::Short => [-32768.0, 32767.0],
            PlyScalar::UShort => [0.0, 65535.0],
            PlyScalar::Int => [i32::MIN as f64, i32::MAX as f64],
            PlyScalar::UInt => [0.0, u32::MAX as f64],
            PlyScalar::Float | PlyScalar::Double => [0.0, 1.0],
        }
    }
}

#[derive(Debug, Clone)]
struct ExtraAttribute {
    name: String,
    scalar: PlyScalar,
}

impl ExtraAttribute {
    fn byte_size(&self) -> usize { self.scalar.byte_size() }
    fn to_metadata_json(&self) -> serde_json::Value {
        let sz = self.byte_size();
        let [vmin, vmax] = self.scalar.value_range();
        serde_json::json!({
            "name": self.name,
            "description": "",
            "size": sz,
            "numElements": 1,
            "elementSize": sz,
            "type": self.scalar.potree_type(),
            "min": [vmin],
            "max": [vmax]
        })
    }
}

#[derive(Debug)]
struct PlyHeader {
    format: PlyFormat,
    vertex_count: usize,
    properties: Vec<(String, PlyScalar)>,
    header_len: u64,
    has_color: bool,
    extra_attributes: Vec<ExtraAttribute>,
}

#[derive(Debug, Clone)]
struct ParsedPoint {
    position: [f64; 3],
    color: Option<[u16; 3]>,
    extra: Vec<u8>,
}

pub fn convert_ply_streaming(
    input: &Path,
    output: &Path,
    name: &str,
    projection: &str,
    target_scale: [f64; 3],
    max_points_per_node: usize,
    max_depth: u32,
    seed: Option<u64>,
) -> Result<(), ConvertError> {
    let header = parse_ply_header(input)?;

    // Pass 1: bbox/spacings
    let pb_bounds = progress_bar(header.vertex_count as u64, "Scanning PLY");
    let (min, max) = pass_compute_bounds(input, &header, Some(&pb_bounds))?;
    pb_bounds.finish_and_clear();
    let total_points = header.vertex_count as u64;
    let (scale, offset) = compute_scale_offset(min, max, target_scale);
    let spacing = estimate_spacing(min, max, total_points);

    // Root node with temp bucket
    let run_id = rand::random::<u64>();
    let extra_size: usize = header.extra_attributes.iter().map(|a| a.byte_size()).sum();
    let record_size = 12 + if header.has_color { 6 } else { 0 } + extra_size;

    let mut nodes: Vec<Node> = Vec::new();
    let mut rng = StdRng::seed_from_u64(seed.unwrap_or(1));
    let mut root_path = std::env::temp_dir();
    root_path.push(format!("potree_{run_id}_r.bin"));
    nodes.push(Node {
        name: "r".to_string(),
        level: 0,
        min,
        max,
        children: [None; 8],
        child_mask: 0,
        num_points: 0,
        byte_offset: 0,
        byte_size: 0,
        temp_path: Some(root_path.clone()),
        sample_data: Vec::new(),
    });

    // Pass 2: stream points into root bucket
    {
        let mut reader = open_after_header(input, &header)?;
        let pb = progress_bar(header.vertex_count as u64, "Ingesting points");
        let mut root_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&root_path)?;
        for _ in 0..header.vertex_count {
            let point = read_point(&mut reader, &header)?;
            let mut record = Vec::with_capacity(record_size);
            write_point_quantized(&mut record, &point, scale, offset)?;
            root_file.write_all(&record)?;
            nodes[0].num_points = nodes[0].num_points.saturating_add(1);
            pb.inc(1);
        }
        pb.finish_and_clear();
    }

    // Build adaptive tree by splitting buckets
    split_tree(
        &mut nodes,
        record_size,
        max_points_per_node,
        max_depth,
        scale,
        offset,
        run_id,
    )?;

    // Bottom-up sampling for internal nodes
    sample_tree(
        &mut nodes,
        record_size,
        max_points_per_node,
        scale,
        offset,
        spacing,
        &mut rng,
    )?;

    // Reorder to preorder for Potree
    let mut nodes = reorder_nodes_preorder(nodes);

    // Write octree.bin in preorder (internal node samples, leaf buckets)
    let mut octree_file = File::create(output.join("octree.bin"))?;
    let mut current_offset = 0u64;
    for node in &mut nodes {
        if node.child_mask == 0 {
            if let Some(path) = &node.temp_path {
                let mut f = File::open(path)?;
                let size = f.metadata()?.len();
                std::io::copy(&mut f, &mut octree_file)?;
                node.byte_offset = current_offset;
                node.byte_size = size;
                current_offset += size;
                let _ = fs::remove_file(path);
            } else {
                node.byte_offset = current_offset;
                node.byte_size = 0;
            }
        } else {
            let size = node.sample_data.len() as u64;
            octree_file.write_all(&node.sample_data)?;
            node.byte_offset = current_offset;
            node.byte_size = size;
            current_offset += size;
        }
    }

    // hierarchy.bin
    let mut hierarchy = Vec::with_capacity(nodes.len() * HIERARCHY_BYTES_PER_NODE);
    for node in &nodes {
        let node_type = if node.child_mask == 0 { 1u8 } else { 0u8 };
        hierarchy.write_u8(node_type)?;
        hierarchy.write_u8(node.child_mask)?;
        hierarchy.write_u32::<LittleEndian>(node.num_points)?;
        hierarchy.write_u64::<LittleEndian>(node.byte_offset)?;
        hierarchy.write_u64::<LittleEndian>(node.byte_size)?;
    }
    fs::write(output.join("hierarchy.bin"), &hierarchy)?;

    let max_level = nodes.iter().map(|n| n.level).max().unwrap_or(0);

    // metadata.json
    let extra_attrs_json: Vec<serde_json::Value> = header.extra_attributes
        .iter()
        .map(|a| a.to_metadata_json())
        .collect();
    let metadata = build_metadata_json(
        name,
        projection,
        total_points,
        min,
        max,
        scale,
        offset,
        spacing,
        "DEFAULT",
        header.has_color,
        max_level,
        hierarchy.len(),
        &extra_attrs_json,
    )?;
    fs::write(output.join("metadata.json"), metadata)?;

    Ok(())
}

fn parse_ply_header(path: &Path) -> Result<PlyHeader, ConvertError> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if !line.trim().eq_ignore_ascii_case("ply") {
        return Err(ConvertError::InvalidInput("Not a PLY file".to_string()));
    }

    line.clear();
    reader.read_line(&mut line)?;
    let format = if line.contains("ascii") {
        PlyFormat::Ascii
    } else if line.contains("binary_little_endian") {
        PlyFormat::BinaryLittleEndian
    } else if line.contains("binary_big_endian") {
        PlyFormat::BinaryBigEndian
    } else {
        return Err(ConvertError::InvalidInput("Unsupported PLY format".to_string()));
    };

    let mut vertex_count = 0usize;
    let mut properties = Vec::new();
    let mut has_color = false;

    loop {
        line.clear();
        reader.read_line(&mut line)?;
        let trimmed = line.trim();
        if trimmed == "end_header" {
            break;
        }
        if trimmed.starts_with("element") {
            let parts: Vec<_> = trimmed.split_whitespace().collect();
            if parts.len() == 3 && parts[1] == "vertex" {
                vertex_count = parts[2]
                    .parse::<usize>()
                    .map_err(|_| ConvertError::InvalidInput("Invalid vertex count".to_string()))?;
            }
        } else if trimmed.starts_with("property") && vertex_count > 0 {
            let parts: Vec<_> = trimmed.split_whitespace().collect();
            if parts.len() == 3 {
                let scalar = match parts[1] {
                    "char" | "int8" => PlyScalar::Char,
                    "uchar" | "uint8" => PlyScalar::UChar,
                    "short" | "int16" => PlyScalar::Short,
                    "ushort" | "uint16" => PlyScalar::UShort,
                    "int" | "int32" => PlyScalar::Int,
                    "uint" | "uint32" => PlyScalar::UInt,
                    "float" | "float32" => PlyScalar::Float,
                    "double" | "float64" => PlyScalar::Double,
                    _ => {
                        return Err(ConvertError::InvalidInput(
                            "Unsupported property type".to_string(),
                        ))
                    }
                };
                let name = parts[2].to_string();
                if matches!(name.as_str(), "red" | "green" | "blue" | "r" | "g" | "b") {
                    has_color = true;
                }
                properties.push((name, scalar));
            }
        }
    }

    let header_len = reader.stream_position()?;

    let extra_attributes: Vec<ExtraAttribute> = properties
        .iter()
        .filter(|(name, _)| !matches!(name.as_str(), "x" | "y" | "z" | "red" | "green" | "blue" | "r" | "g" | "b"))
        .map(|(name, scalar)| ExtraAttribute { name: name.clone(), scalar: *scalar })
        .collect();

    Ok(PlyHeader {
        format,
        vertex_count,
        properties,
        header_len,
        has_color,
        extra_attributes,
    })
}

fn open_after_header(path: &Path, header: &PlyHeader) -> Result<BufReader<File>, ConvertError> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(header.header_len))?;
    Ok(BufReader::new(file))
}

fn pass_compute_bounds(
    path: &Path,
    header: &PlyHeader,
    progress: Option<&ProgressBar>,
) -> Result<([f64; 3], [f64; 3]), ConvertError> {
    let mut reader = open_after_header(path, header)?;
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for _ in 0..header.vertex_count {
        let point = read_point(&mut reader, header)?;
        for i in 0..3 {
            min[i] = min[i].min(point.position[i]);
            max[i] = max[i].max(point.position[i]);
        }
        if let Some(pb) = progress {
            pb.inc(1);
        }
    }
    Ok((min, max))
}

fn read_point<R: BufRead + Read>(reader: &mut R, header: &PlyHeader) -> Result<ParsedPoint, ConvertError> {
    match header.format {
        PlyFormat::Ascii => read_point_ascii(reader, header),
        PlyFormat::BinaryLittleEndian => read_point_binary::<_, LittleEndian>(reader, header),
        PlyFormat::BinaryBigEndian => read_point_binary::<_, BigEndian>(reader, header),
    }
}

fn sample_tree(
    nodes: &mut Vec<Node>,
    record_size: usize,
    _max_points_per_node: usize,
    scale: [f64; 3],
    offset: [f64; 3],
    base_spacing: f64,
    _rng: &mut StdRng,
) -> Result<(), ConvertError> {
    // Build postorder traversal order (leaves first, root last).
    let mut order: Vec<usize> = Vec::new();
    let mut stack = vec![0usize];
    while let Some(idx) = stack.pop() {
        order.push(idx);
        for child in nodes[idx].children.iter().flatten() {
            stack.push(*child);
        }
    }

    for &idx in order.iter().rev() {
        if nodes[idx].child_mask == 0 {
            // Leaf: payload stays as-is; num_points already set during bucket fill.
            continue;
        }

        let node_min = nodes[idx].min;
        let node_max = nodes[idx].max;
        let center = [
            (node_min[0] + node_max[0]) * 0.5,
            (node_min[1] + node_max[1]) * 0.5,
            (node_min[2] + node_max[2]) * 0.5,
        ];
        let spacing = base_spacing / 2f64.powi(nodes[idx].level as i32);

        // Stable list of child node indices for this internal node.
        let child_indices: Vec<usize> = nodes[idx].children.iter().flatten().copied().collect();

        // Read the current raw-byte payload of each child.
        let child_raw: Vec<Vec<Vec<u8>>> = child_indices
            .iter()
            .map(|&ci| read_raw_records_from_node(&nodes[ci], record_size))
            .collect::<Result<_, _>>()?;

        // Flatten to positions + source tags for Poisson sampling.
        let mut all_positions: Vec<[f64; 3]> = Vec::new();
        let mut record_tags: Vec<(usize, usize)> = Vec::new(); // (child_list_idx, record_idx)
        for (ci, records) in child_raw.iter().enumerate() {
            for (ri, record) in records.iter().enumerate() {
                let pt = decode_position(record, scale, offset);
                all_positions.push(pt);
                record_tags.push((ci, ri));
            }
        }

        // Determine which points are accepted into this node's LOD payload.
        let flags = poisson_accept(&all_positions, spacing, center);

        // Partition: accepted → this node's sample_data; rejected → back to child.
        let mut sample_data: Vec<u8> = Vec::new();
        let mut per_child_rejected: Vec<Vec<Vec<u8>>> =
            child_indices.iter().map(|_| Vec::new()).collect();
        for (i, &(ci, ri)) in record_tags.iter().enumerate() {
            if flags[i] {
                sample_data.extend_from_slice(&child_raw[ci][ri]);
            } else {
                per_child_rejected[ci].push(child_raw[ci][ri].clone());
            }
        }

        // Write back only the rejected records to each child, preserving the
        // invariant that every input point lives in exactly one node's payload.
        for (ci, &child_idx) in child_indices.iter().enumerate() {
            let rejected = std::mem::take(&mut per_child_rejected[ci]);
            write_rejected_to_node(&mut nodes[child_idx], rejected)?;
        }

        nodes[idx].sample_data = sample_data;
        nodes[idx].num_points = (nodes[idx].sample_data.len() / record_size) as u32;
    }

    Ok(())
}

fn read_point_ascii<R: BufRead + Read>(reader: &mut R, header: &PlyHeader) -> Result<ParsedPoint, ConvertError> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if line.is_empty() {
        return Err(ConvertError::InvalidInput("Unexpected EOF".to_string()));
    }
    let parts: Vec<_> = line.split_whitespace().collect();
    if parts.len() < header.properties.len() {
        return Err(ConvertError::InvalidInput(
            "Incomplete vertex line".to_string(),
        ));
    }
    let mut pos = [0f64; 3];
    let mut color: Option<[u16; 3]> = None;
    let mut extra: Vec<u8> = Vec::new();
    let extra_names: HashSet<&str> = header.extra_attributes.iter().map(|a| a.name.as_str()).collect();
    for (i, (name, ty)) in header.properties.iter().enumerate() {
        let val = parts[i];
        let v = parse_scalar_ascii(val, ty)?;
        match name.as_str() {
            "x" => pos[0] = v,
            "y" => pos[1] = v,
            "z" => pos[2] = v,
            "red" | "r" => {
                let r = v.round().clamp(0.0, 255.0) as u16 * 257;
                color.get_or_insert([0, 0, 0])[0] = r;
            }
            "green" | "g" => {
                let g = v.round().clamp(0.0, 255.0) as u16 * 257;
                color.get_or_insert([0, 0, 0])[1] = g;
            }
            "blue" | "b" => {
                let b = v.round().clamp(0.0, 255.0) as u16 * 257;
                color.get_or_insert([0, 0, 0])[2] = b;
            }
            _ => {
                if extra_names.contains(name.as_str()) {
                    let sz = ty.byte_size();
                    let mut buf = [0u8; 8];
                    match ty {
                        PlyScalar::Char   => buf[0] = v as i8 as u8,
                        PlyScalar::UChar  => buf[0] = v as u8,
                        PlyScalar::Short  => LittleEndian::write_i16(&mut buf[..2], v as i16),
                        PlyScalar::UShort => LittleEndian::write_u16(&mut buf[..2], v as u16),
                        PlyScalar::Int    => LittleEndian::write_i32(&mut buf[..4], v as i32),
                        PlyScalar::UInt   => LittleEndian::write_u32(&mut buf[..4], v as u32),
                        PlyScalar::Float  => LittleEndian::write_f32(&mut buf[..4], v as f32),
                        PlyScalar::Double => LittleEndian::write_f64(&mut buf[..8], v),
                    }
                    extra.extend_from_slice(&buf[..sz]);
                }
            }
        }
    }
    Ok(ParsedPoint { position: pos, color, extra })
}

fn parse_scalar_ascii(val: &str, _ty: &PlyScalar) -> Result<f64, ConvertError> {
    let v = val
        .parse::<f64>()
        .map_err(|_| ConvertError::InvalidInput(format!("Unable to parse value {}", val)))?;
    Ok(v)
}

fn read_point_binary<R: Read, BO: ByteOrder>(reader: &mut R, header: &PlyHeader) -> Result<ParsedPoint, ConvertError> {
    let mut pos = [0f64; 3];
    let mut color: Option<[u16; 3]> = None;
    let mut extra: Vec<u8> = Vec::new();
    let extra_names: HashSet<&str> = header.extra_attributes.iter().map(|a| a.name.as_str()).collect();
    for (name, ty) in &header.properties {
        let v = match ty {
            PlyScalar::Char => reader.read_i8()? as f64,
            PlyScalar::UChar => reader.read_u8()? as f64,
            PlyScalar::Short => reader.read_i16::<BO>()? as f64,
            PlyScalar::UShort => reader.read_u16::<BO>()? as f64,
            PlyScalar::Int => reader.read_i32::<BO>()? as f64,
            PlyScalar::UInt => reader.read_u32::<BO>()? as f64,
            PlyScalar::Float => reader.read_f32::<BO>()? as f64,
            PlyScalar::Double => reader.read_f64::<BO>()?,
        };
        match name.as_str() {
            "x" => pos[0] = v,
            "y" => pos[1] = v,
            "z" => pos[2] = v,
            "red" | "r" => {
                let r = (v.round().clamp(0.0, 255.0) as u16) * 257;
                color.get_or_insert([0, 0, 0])[0] = r;
            }
            "green" | "g" => {
                let g = (v.round().clamp(0.0, 255.0) as u16) * 257;
                color.get_or_insert([0, 0, 0])[1] = g;
            }
            "blue" | "b" => {
                let b = (v.round().clamp(0.0, 255.0) as u16) * 257;
                color.get_or_insert([0, 0, 0])[2] = b;
            }
            _ => {
                if extra_names.contains(name.as_str()) {
                    let sz = ty.byte_size();
                    let mut buf = [0u8; 8];
                    match ty {
                        PlyScalar::Char   => buf[0] = v as i8 as u8,
                        PlyScalar::UChar  => buf[0] = v as u8,
                        PlyScalar::Short  => LittleEndian::write_i16(&mut buf[..2], v as i16),
                        PlyScalar::UShort => LittleEndian::write_u16(&mut buf[..2], v as u16),
                        PlyScalar::Int    => LittleEndian::write_i32(&mut buf[..4], v as i32),
                        PlyScalar::UInt   => LittleEndian::write_u32(&mut buf[..4], v as u32),
                        PlyScalar::Float  => LittleEndian::write_f32(&mut buf[..4], v as f32),
                        PlyScalar::Double => LittleEndian::write_f64(&mut buf[..8], v),
                    }
                    extra.extend_from_slice(&buf[..sz]);
                }
            }
        }
    }
    Ok(ParsedPoint { position: pos, color, extra })
}

fn split_tree(
    nodes: &mut Vec<Node>,
    record_size: usize,
    max_points_per_node: usize,
    max_depth: u32,
    scale: [f64; 3],
    offset: [f64; 3],
    run_id: u64,
) -> Result<(), ConvertError> {
    let mut queue: VecDeque<usize> = VecDeque::new();
    queue.push_back(0);

    while let Some(idx) = queue.pop_front() {
        if nodes[idx].num_points as usize <= max_points_per_node || nodes[idx].level >= max_depth {
            continue;
        }

        let node_min = nodes[idx].min;
        let node_max = nodes[idx].max;

        // ensure children
        for child in 0u8..8 {
            let child_idx = child as usize;
            if nodes[idx].children[child_idx].is_none() {
                let (cmin, cmax) = child_bounds(node_min, node_max, child);
                let new_idx = nodes.len();
                nodes.push(Node {
                    name: format!("{}{}", nodes[idx].name, child),
                    level: nodes[idx].level + 1,
                    min: cmin,
                    max: cmax,
                    children: [None; 8],
                    child_mask: 0,
                    num_points: 0,
                    byte_offset: 0,
                    byte_size: 0,
                    temp_path: None,
                    sample_data: Vec::new(),
                });
                nodes[idx].children[child_idx] = Some(new_idx);
                nodes[idx].child_mask |= 1 << child;
            }
        }

        // redistribute this node's payload to children
        if let Some(path) = nodes[idx].temp_path.take() {
            let mut f = File::open(&path)?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            fs::remove_file(path)?;

            let mut pos = 0;
            while pos + record_size <= buf.len() {
                let record = &buf[pos..pos + record_size];
                pos += record_size;
                let point = decode_position(record, scale, offset);

                let center = [
                    (node_min[0] + node_max[0]) * 0.5,
                    (node_min[1] + node_max[1]) * 0.5,
                    (node_min[2] + node_max[2]) * 0.5,
                ];
                let mut child = 0u8;
                if point[2] >= center[2] {
                    child |= 0b0001;
                }
                if point[1] >= center[1] {
                    child |= 0b0010;
                }
                if point[0] >= center[0] {
                    child |= 0b0100;
                }
                let child_idx = nodes[idx].children[child as usize].unwrap();
                ensure_leaf_bucket(nodes, child_idx, run_id)?;
                let child_path = nodes[child_idx].temp_path.as_ref().unwrap();
                let mut cf = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(child_path)?;
                cf.write_all(record)?;
                nodes[child_idx].num_points = nodes[child_idx].num_points.saturating_add(1);
            }
        }

        // clear this node's payload
        nodes[idx].temp_path = None;
        nodes[idx].num_points = 0;

        // enqueue children
        for child in nodes[idx].children.iter().flatten() {
            queue.push_back(*child);
        }
    }

    Ok(())
}

fn ensure_leaf_bucket(nodes: &mut Vec<Node>, idx: usize, run_id: u64) -> Result<(), ConvertError> {
    if nodes[idx].temp_path.is_none() {
        let mut p = std::env::temp_dir();
        p.push(format!("potree_{}_{}.bin", run_id, nodes[idx].name));
        nodes[idx].temp_path = Some(p);
    }
    Ok(())
}

/// Read all raw point records from a node's current payload.
/// - Leaves: reads from the temp file on disk.
/// - Internal nodes: reads from `sample_data` (their already-computed LOD payload).
fn read_raw_records_from_node(node: &Node, record_size: usize) -> Result<Vec<Vec<u8>>, ConvertError> {
    let slice: &[u8] = if node.child_mask == 0 {
        if let Some(path) = &node.temp_path {
            let mut f = File::open(path)?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            return Ok(buf.chunks_exact(record_size).map(|c| c.to_vec()).collect());
        }
        return Ok(Vec::new());
    } else {
        &node.sample_data
    };
    Ok(slice.chunks_exact(record_size).map(|c| c.to_vec()).collect())
}

/// Write the rejected records back into a node's payload after the parent has
/// taken the accepted points for its LOD sample.
/// - Leaves: overwrites the temp file (removes it when empty).
/// - Internal nodes: replaces `sample_data`.
fn write_rejected_to_node(node: &mut Node, rejected: Vec<Vec<u8>>) -> Result<(), ConvertError> {
    let n = rejected.len() as u32;
    if node.child_mask == 0 {
        if n == 0 {
            if let Some(path) = node.temp_path.take() {
                let _ = fs::remove_file(&path);
            }
        } else {
            let path = node
                .temp_path
                .get_or_insert_with(|| {
                    let mut p = std::env::temp_dir();
                    p.push(format!("potree_node_{}.bin", node.name));
                    p
                })
                .clone();
            let mut f = File::create(&path)?;
            for r in &rejected {
                f.write_all(r)?;
            }
        }
    } else {
        node.sample_data = rejected.into_iter().flatten().collect();
    }
    node.num_points = n;
    Ok(())
}

/// Core Poisson-disk acceptance algorithm matching PotreeConverter `SamplerPoisson`.
///
/// Returns `flags[i] = true` when `positions[i]` is accepted (not masked by
/// any closer-to-center accepted point within `spacing`).
///
/// Algorithm:
/// - Sort positions by distance-to-`center` ascending (closest processed first).
/// - For each candidate, scan already-accepted list backwards (furthest first).
///   - Early-exit when an accepted point's center-dist² < `(cd - spacing)²`
///     (no earlier accepted point can be within `spacing` of the candidate).
///   - Hard cap of 10 000 backward-scan steps per candidate.
fn poisson_accept(positions: &[[f64; 3]], spacing: f64, center: [f64; 3]) -> Vec<bool> {
    if positions.is_empty() {
        return Vec::new();
    }

    let mut order: Vec<(usize, f64)> = positions
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let d = (p[0] - center[0]).powi(2)
                + (p[1] - center[1]).powi(2)
                + (p[2] - center[2]).powi(2);
            (i, d)
        })
        .collect();
    order.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let spacing_sq = spacing * spacing;
    let mut accepted_positions: Vec<[f64; 3]> = Vec::new();
    let mut flags = vec![false; positions.len()];

    'outer: for (orig_idx, _) in &order {
        let pos = positions[*orig_idx];
        let cx = pos[0] - center[0];
        let cy = pos[1] - center[1];
        let cz = pos[2] - center[2];
        let cd = (cx * cx + cy * cy + cz * cz).sqrt();
        let limit = cd - spacing;
        let limit_sq = limit * limit;

        let mut checks = 0usize;
        for ap in accepted_positions.iter().rev() {
            let px = ap[0] - center[0];
            let py = ap[1] - center[1];
            let pz = ap[2] - center[2];
            let pdd = px * px + py * py + pz * pz;
            if pdd < limit_sq {
                break; // early exit: all remaining accepted points are even closer → safe
            }
            let dx = ap[0] - pos[0];
            let dy = ap[1] - pos[1];
            let dz = ap[2] - pos[2];
            if dx * dx + dy * dy + dz * dz < spacing_sq {
                continue 'outer; // too close → reject candidate
            }
            checks += 1;
            if checks >= 10_000 {
                break;
            }
        }

        flags[*orig_idx] = true;
        accepted_positions.push(pos);
    }

    flags
}

/// Poisson-disk sample `points` for an internal octree node.
/// Uses [`poisson_accept`] and re-encodes accepted points into raw records.
#[cfg(test)]
fn poisson_sample(
    points: &[ParsedPoint],
    record_size: usize,
    spacing: f64,
    scale: [f64; 3],
    offset: [f64; 3],
    node_min: [f64; 3],
    node_max: [f64; 3],
) -> Result<Vec<u8>, ConvertError> {
    if points.is_empty() {
        return Ok(Vec::new());
    }

    let center = [
        (node_min[0] + node_max[0]) * 0.5,
        (node_min[1] + node_max[1]) * 0.5,
        (node_min[2] + node_max[2]) * 0.5,
    ];
    let positions: Vec<[f64; 3]> = points.iter().map(|p| p.position).collect();
    let flags = poisson_accept(&positions, spacing, center);

    let mut out = Vec::new();
    for (p, &accepted) in points.iter().zip(flags.iter()) {
        if accepted {
            let mut record = Vec::with_capacity(record_size);
            write_point_quantized(&mut record, p, scale, offset)?;
            out.extend_from_slice(&record);
        }
    }
    Ok(out)
}

fn decode_position(record: &[u8], scale: [f64; 3], offset: [f64; 3]) -> [f64; 3] {
    let x = LittleEndian::read_i32(&record[0..4]) as f64 * scale[0] + offset[0];
    let y = LittleEndian::read_i32(&record[4..8]) as f64 * scale[1] + offset[1];
    let z = LittleEndian::read_i32(&record[8..12]) as f64 * scale[2] + offset[2];
    [x, y, z]
}

fn child_bounds(min: [f64; 3], max: [f64; 3], index: u8) -> ([f64; 3], [f64; 3]) {
    let mut child_min = min;
    let mut child_max = max;
    let size = [
        (max[0] - min[0]) * 0.5,
        (max[1] - min[1]) * 0.5,
        (max[2] - min[2]) * 0.5,
    ];

    if (index & 0b0001) > 0 {
        child_min[2] += size[2];
    } else {
        child_max[2] -= size[2];
    }
    if (index & 0b0010) > 0 {
        child_min[1] += size[1];
    } else {
        child_max[1] -= size[1];
    }
    if (index & 0b0100) > 0 {
        child_min[0] += size[0];
    } else {
        child_max[0] -= size[0];
    }

    (child_min, child_max)
}

fn reorder_nodes_preorder(nodes: Vec<Node>) -> Vec<Node> {
    let mut new_nodes = Vec::with_capacity(nodes.len());
    let mut mapping = vec![usize::MAX; nodes.len()];
    let mut stack = Vec::new();
    stack.push(0usize);

    while let Some(old_idx) = stack.pop() {
        if mapping[old_idx] != usize::MAX {
            continue;
        }
        let new_idx = new_nodes.len();
        mapping[old_idx] = new_idx;
        new_nodes.push(nodes[old_idx].clone());

        // push children in reverse order to visit 0..7 in preorder
        for child in (0..8).rev() {
            if let Some(child_idx) = nodes[old_idx].children[child] {
                stack.push(child_idx);
            }
        }
    }

    // remap children/masks
    for old_idx in 0..nodes.len() {
        let new_idx = mapping[old_idx];
        let mut new_children = [None; 8];
        let mut mask = 0u8;
        for (i, child) in nodes[old_idx].children.iter().enumerate() {
            if let Some(old_child) = child {
                let mapped = mapping[*old_child];
                new_children[i] = Some(mapped);
                mask |= 1 << i;
            }
        }
        new_nodes[new_idx].children = new_children;
        new_nodes[new_idx].child_mask = mask;
    }

    new_nodes
}

fn write_point_quantized<W: Write>(
    writer: &mut W,
    point: &ParsedPoint,
    scale: [f64; 3],
    offset: [f64; 3],
) -> Result<(), ConvertError> {
    let ix = quantize_i32(point.position[0], scale[0], offset[0]);
    let iy = quantize_i32(point.position[1], scale[1], offset[1]);
    let iz = quantize_i32(point.position[2], scale[2], offset[2]);

    writer.write_i32::<LittleEndian>(ix)?;
    writer.write_i32::<LittleEndian>(iy)?;
    writer.write_i32::<LittleEndian>(iz)?;

    if let Some([r, g, b]) = point.color {
        writer.write_u16::<LittleEndian>(r)?;
        writer.write_u16::<LittleEndian>(g)?;
        writer.write_u16::<LittleEndian>(b)?;
    }
    writer.write_all(&point.extra)?;
    Ok(())
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

fn progress_bar(len: u64, msg: &str) -> ProgressBar {
    let pb = ProgressBar::new(len);
    let style = ProgressStyle::with_template(
        "{msg} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} eta:{eta}",
    )
    .unwrap_or_else(|_| ProgressStyle::default_bar())
    .progress_chars("##-");
    pb.set_style(style);
    pb.set_message(msg.to_string());
    pb
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f64, y: f64, z: f64) -> ParsedPoint {
        ParsedPoint { position: [x, y, z], color: None, extra: vec![] }
    }

    // Decode the xyz from a 12-byte (no-color) record.
    fn decode_xyz(record: &[u8], scale: [f64; 3], offset: [f64; 3]) -> [f64; 3] {
        let x = LittleEndian::read_i32(&record[0..4]) as f64 * scale[0] + offset[0];
        let y = LittleEndian::read_i32(&record[4..8]) as f64 * scale[1] + offset[1];
        let z = LittleEndian::read_i32(&record[8..12]) as f64 * scale[2] + offset[2];
        [x, y, z]
    }

    #[test]
    fn poisson_accept_empty() {
        let flags = poisson_accept(&[], 1.0, [0.0; 3]);
        assert!(flags.is_empty());
    }

    #[test]
    fn poisson_accept_flags_count_matches_input() {
        let positions = vec![[0.0f64; 3], [10.0, 0.0, 0.0], [20.0, 0.0, 0.0]];
        let flags = poisson_accept(&positions, 1.0, [10.0, 0.0, 0.0]);
        assert_eq!(flags.len(), 3);
    }

    #[test]
    fn poisson_accept_preserves_original_index_order() {
        // Three points: only the one nearest geometric center should be accepted.
        // Center [5,5,5], points at x=8.0 (cd=3) and x=8.3 (cd=3.3), spacing=0.5.
        // [8.0] (cd=3) → accepted first; [8.3] too close → rejected.
        let positions = vec![[8.3f64, 5.0, 5.0], [8.0, 5.0, 5.0]]; // note: 8.3 is index 0
        let flags = poisson_accept(&positions, 0.5, [5.0, 5.0, 5.0]);
        assert_eq!(flags.len(), 2);
        assert!(!flags[0], "[8.3] (farther from center) should be rejected");
        assert!(flags[1], "[8.0] (closer to center) should be accepted");
    }

    #[test]
    fn poisson_empty_input() {
        let out = poisson_sample(
            &[],
            12,
            1.0,
            [0.001; 3],
            [0.0; 3],
            [0.0; 3],
            [1.0; 3],
        )
        .unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn poisson_single_point_always_accepted() {
        let points = vec![pt(0.5, 0.5, 0.5)];
        let out = poisson_sample(
            &points,
            12,
            1.0,
            [0.001; 3],
            [0.0; 3],
            [0.0; 3],
            [1.0; 3],
        )
        .unwrap();
        assert_eq!(out.len(), 12);
    }

    #[test]
    fn poisson_two_close_points_one_rejected() {
        // Points must be FAR from center (cd > spacing) for rejection to work.
        // spacing = 0.5, node [0,10]^3, center=[5,5,5]
        // Both points near x=8 (cd≈3 >> 0.5), 0.3 apart → second is rejected.
        let points = vec![pt(8.0, 5.0, 5.0), pt(8.3, 5.0, 5.0)];
        let out = poisson_sample(
            &points,
            12,
            0.5,
            [0.001; 3],
            [0.0; 3],
            [0.0; 3],
            [10.0; 3],
        )
        .unwrap();
        assert_eq!(out.len() / 12, 1);
    }

    #[test]
    fn poisson_far_apart_points_both_accepted() {
        // Two points 2.0 apart, spacing = 1.0 → both accepted
        let points = vec![pt(0.0, 0.0, 0.0), pt(2.0, 0.0, 0.0)];
        let scale = [0.001; 3];
        let offset = [0.0; 3];
        let out = poisson_sample(
            &points,
            12,
            1.0,
            scale,
            offset,
            [-1.0; 3],
            [3.0; 3],
        )
        .unwrap();
        assert_eq!(out.len() / 12, 2);
    }

    #[test]
    fn poisson_minimum_distance_invariant() {
        // 100 points on a regular grid with spacing 0.1; poisson spacing = 0.25
        // All accepted pairs must be at least 0.25 apart (within quantization error).
        let mut points = Vec::new();
        for ix in 0..10 {
            for iy in 0..10 {
                points.push(pt(ix as f64 * 0.1, iy as f64 * 0.1, 0.0));
            }
        }
        let scale = [0.001; 3];
        let offset = [0.0; 3];
        let spacing = 0.25;
        let out = poisson_sample(
            &points,
            12,
            spacing,
            scale,
            offset,
            [-0.1; 3],
            [1.1; 3],
        )
        .unwrap();

        let n = out.len() / 12;
        assert!(n > 0);

        // Decode accepted points and verify minimum distance
        let accepted_xyz: Vec<[f64; 3]> = (0..n)
            .map(|i| decode_xyz(&out[i * 12..(i + 1) * 12], scale, offset))
            .collect();

        for i in 0..accepted_xyz.len() {
            for j in (i + 1)..accepted_xyz.len() {
                let a = accepted_xyz[i];
                let b = accepted_xyz[j];
                let d = ((a[0]-b[0]).powi(2)+(a[1]-b[1]).powi(2)+(a[2]-b[2]).powi(2)).sqrt();
                // allow small quantization slop (1 scale unit = 0.001)
                assert!(
                    d >= spacing - 0.002,
                    "points too close: {d:.4} < {spacing} (i={i}, j={j})"
                );
            }
        }
    }

    #[test]
    fn poisson_uses_geometric_center_not_centroid() {
        // Node bbox [0, 100]^3 → geometric center = [50,50,50].
        // Two points both near x=80, 0.2 apart with spacing=0.3.
        // Both have cd >> spacing so the C++ early-exit won't misfire.
        // [80,50,50] has cd=30 (closer to geom center) → accepted first.
        // [80.2,50,50] has cd=30.2 → checked against [80]: dist=0.2 < 0.3 → rejected.
        // So only 1 point accepted, and its x ≈ 80.0 (not 80.2).
        let points = vec![pt(80.0, 50.0, 50.0), pt(80.2, 50.0, 50.0)];
        let scale = [0.001; 3];
        let offset = [0.0; 3];
        let out = poisson_sample(
            &points,
            12,
            0.3,
            scale,
            offset,
            [0.0; 3],
            [100.0; 3],
        )
        .unwrap();
        assert_eq!(out.len() / 12, 1);
        let xyz = decode_xyz(&out[0..12], scale, offset);
        assert!((xyz[0] - 80.0).abs() < 0.002, "expected x≈80.0 (closer to geom center), got {}", xyz[0]);
    }
}
