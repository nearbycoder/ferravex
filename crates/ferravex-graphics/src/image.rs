use crate::Framebuffer;
impl Framebuffer {
    /// Binary RGB PPM, portable and intentionally uncompressed.
    pub fn to_ppm(&self) -> Vec<u8> {
        let mut output = format!("P6\n{} {}\n255\n", self.width, self.height).into_bytes();
        output.reserve(self.pixels().len() * 3);
        for pixel in self.pixels() {
            output.extend_from_slice(&pixel[..3]);
        }
        output
    }
    /// RGBA PNG with original CRC/Adler and uncompressed DEFLATE blocks.
    /// This trades file size for a small dependency-free, browser-viewable export.
    pub fn to_png(&self) -> Vec<u8> {
        let mut filtered = Vec::with_capacity(self.pixels().len() * 4 + self.height as usize);
        for row in self.pixels().chunks_exact(self.width as usize) {
            filtered.push(0);
            for pixel in row {
                filtered.extend_from_slice(pixel);
            }
        }
        let mut zlib = vec![0x78, 0x01];
        let block_count = filtered.len().div_ceil(65535);
        for (index, block) in filtered.chunks(65535).enumerate() {
            zlib.push(u8::from(index + 1 == block_count));
            let length = block.len() as u16;
            zlib.extend_from_slice(&length.to_le_bytes());
            zlib.extend_from_slice(&(!length).to_le_bytes());
            zlib.extend_from_slice(block);
        }
        zlib.extend_from_slice(&adler32(&filtered).to_be_bytes());
        let mut output = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut header = Vec::with_capacity(13);
        header.extend_from_slice(&self.width.to_be_bytes());
        header.extend_from_slice(&self.height.to_be_bytes());
        header.extend([8, 6, 0, 0, 0]);
        chunk(&mut output, b"IHDR", &header);
        chunk(&mut output, b"IDAT", &zlib);
        chunk(&mut output, b"IEND", &[]);
        output
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_png(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        std::fs::write(path, self.to_png())
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_ppm(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        std::fs::write(path, self.to_ppm())
    }
}
fn adler32(bytes: &[u8]) -> u32 {
    let mut a = 1_u32;
    let mut b = 0_u32;
    for byte in bytes {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
fn crc32(bytes: impl Iterator<Item = u8>) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    !crc
}
fn chunk(output: &mut Vec<u8>, kind: &[u8; 4], bytes: &[u8]) {
    output.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(bytes);
    output.extend_from_slice(&crc32(kind.iter().chain(bytes).copied()).to_be_bytes());
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checksum_known_vectors() {
        assert_eq!(crc32(b"123456789".iter().copied()), 0xcbf43926);
        assert_eq!(adler32(b"Wikipedia"), 0x11e60398);
    }
}
