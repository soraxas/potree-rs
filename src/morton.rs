use byteorder::{ByteOrder, LittleEndian};

pub fn read_morton_64(bytes: &[u8]) -> (u16, u16, u16) {
    let mc_0 = LittleEndian::read_u32(&bytes[4..8]);
    let mc_1 = LittleEndian::read_u32(&bytes[0..4]);

    decode_morton_64(mc_0, mc_1)
}


pub fn read_morton_128(bytes: &[u8]) -> (u32, u32, u32) {
    let mc_0 = LittleEndian::read_u32(&bytes[4..8]);
    let mc_1 = LittleEndian::read_u32(&bytes[0..4]);
    let mc_2 = LittleEndian::read_u32(&bytes[12..16]);
    let mc_3 = LittleEndian::read_u32(&bytes[8..12]);

    decode_morton_128(mc_0, mc_1, mc_2, mc_3)
}

pub fn dealign_24b(mut morton: u32) -> u32 {
    // Keep only 3rd bit
    morton &= 0x09249249; // 0b001001001001001001001001001001

    morton = (morton | (morton >> 2)) & 0x030c30c3;
    morton = (morton | (morton >> 4)) & 0x0300f00f;
    morton = (morton | (morton >> 8)) & 0x030000ff;
    morton = (morton | (morton >> 16)) & 0x000003ff;

    morton
}

pub fn decode_morton_64(mc_0: u32, mc_1: u32) -> (u16, u16, u16) {
    let r = dealign_24b((mc_1 & 0x00FFFFFF) >> 0)
        | (dealign_24b(((mc_1 >> 24) | (mc_0 << 8)) >> 0) << 8);

    let g = dealign_24b((mc_1 & 0x00FFFFFF) >> 1)
        | (dealign_24b(((mc_1 >> 24) | (mc_0 << 8)) >> 1) << 8);

    let b = dealign_24b((mc_1 & 0x00FFFFFF) >> 2)
        | (dealign_24b(((mc_1 >> 24) | (mc_0 << 8)) >> 2) << 8);

    (r as u16, g as u16, b as u16)
}

pub fn decode_morton_128(mc_0: u32, mc_1: u32, mc_2: u32, mc_3: u32) -> (u32, u32, u32) {
    // First part (lower bits)
    let mut x = dealign_24b((mc_3 & 0x00FFFFFF) >> 0)
        | (dealign_24b(((mc_3 >> 24) | (mc_2 << 8)) >> 0) << 8);

    let mut y = dealign_24b((mc_3 & 0x00FFFFFF) >> 1)
        | (dealign_24b(((mc_3 >> 24) | (mc_2 << 8)) >> 1) << 8);

    let mut z = dealign_24b((mc_3 & 0x00FFFFFF) >> 2)
        | (dealign_24b(((mc_3 >> 24) | (mc_2 << 8)) >> 2) << 8);

    // Second part (upper bits) - only if needed
    if mc_1 != 0 || mc_2 != 0 {
        x |= (dealign_24b((mc_1 & 0x00FFFFFF) >> 0) << 16)
            | (dealign_24b(((mc_1 >> 24) | (mc_0 << 8)) >> 0) << 24);

        y |= (dealign_24b((mc_1 & 0x00FFFFFF) >> 1) << 16)
            | (dealign_24b(((mc_1 >> 24) | (mc_0 << 8)) >> 1) << 24);

        z |= (dealign_24b((mc_1 & 0x00FFFFFF) >> 2) << 16)
            | (dealign_24b(((mc_1 >> 24) | (mc_0 << 8)) >> 2) << 24);
    }

    (x, y, z)
}
