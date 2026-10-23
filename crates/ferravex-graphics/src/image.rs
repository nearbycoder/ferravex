use crate::Framebuffer;

/// Invalid input to a checked framebuffer color-region operation.
///
/// Errors leave the destination's colors and depths unchanged.
///
/// ```
/// use ferravex_graphics::Framebuffer;
///
/// let mut atlas = Framebuffer::new(4, 4).unwrap();
/// let error = atlas.blit_rgba(0, 0, 2, 2, &[[255; 4]; 3], 2).unwrap_err();
/// assert_eq!(error.to_string(), "image region needs 4 source pixels, but only 3 are available");
/// let _: &dyn std::error::Error = &error;
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageRegionError {
    /// Width or height is zero or exceeds framebuffer allocation limits.
    InvalidDimensions,
    /// The entire region must fit within the framebuffer; clipping is not performed.
    OutOfBounds,
    /// The source row stride, measured in RGBA pixels, is smaller than the region width.
    InvalidStride,
    /// The source layout's last-row end cannot be represented by `usize`.
    LayoutOverflow,
    /// The source slice does not reach the end of the final copied row.
    SourceTooShort {
        /// Minimum source slice length, in RGBA pixels, including intervening padding.
        required: usize,
        /// Supplied source slice length, in RGBA pixels.
        actual: usize,
    },
}

impl std::fmt::Display for ImageRegionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDimensions => f.write_str("invalid image region dimensions"),
            Self::OutOfBounds => f.write_str("image region is outside the framebuffer"),
            Self::InvalidStride => {
                f.write_str("image region source stride is smaller than its width")
            }
            Self::LayoutOverflow => f.write_str("image region source layout overflows usize"),
            Self::SourceTooShort { required, actual } => write!(
                f,
                "image region needs {required} source pixels, but only {actual} are available"
            ),
        }
    }
}

impl std::error::Error for ImageRegionError {}

impl Framebuffer {
    /// Copies a color rectangle into a new, tightly packed framebuffer.
    ///
    /// `(x, y)` is the top-left pixel; rows proceed downward. Width and height
    /// must be nonzero, and the entire rectangle must fit; there is no clipping.
    /// Every RGBA byte is copied verbatim, including RGB under zero alpha, with
    /// no blending, premultiplication or color-space conversion. The source is
    /// unchanged and the new framebuffer's depth values start at positive infinity.
    /// Allocation is bounded by the existing framebuffer dimension/pixel limits.
    ///
    /// # Errors
    ///
    /// Returns [`ImageRegionError::InvalidDimensions`] for an empty rectangle,
    /// or [`ImageRegionError::OutOfBounds`] if it extends outside the source.
    /// Invalid rectangles are rejected before allocation.
    ///
    /// ```
    /// use ferravex_graphics::Framebuffer;
    ///
    /// let mut atlas = Framebuffer::new(4, 3).unwrap();
    /// atlas.pixels_mut()[6] = [200, 100, 50, 0];
    /// let tile = atlas.crop(2, 1, 2, 2).unwrap();
    /// assert_eq!((tile.width(), tile.height()), (2, 2));
    /// assert_eq!(tile.pixel(0, 0), Some([200, 100, 50, 0]));
    /// assert!(tile.depths().iter().all(|&depth| depth == f32::INFINITY));
    /// assert!(atlas.crop(u32::MAX, 0, 2, 2).is_err());
    /// ```
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> Result<Self, ImageRegionError> {
        self.validate_region(x, y, width, height)?;
        // A contained region inherits the source's dimension and pixel bounds.
        let mut cropped =
            Self::new(width, height).map_err(|_| ImageRegionError::InvalidDimensions)?;
        let stride = self.width() as usize;
        let start = y as usize * stride + x as usize;
        cropped.blit_rgba(0, 0, width, height, &self.pixels()[start..], stride)?;
        Ok(cropped)
    }

    /// Replaces a color rectangle from row-major RGBA pixels without allocating.
    ///
    /// `(x, y)` is the destination's top-left pixel; rows proceed downward.
    /// Width and height must be nonzero and fit completely in the destination.
    /// `source[0]` is the source region's top-left pixel. `source_stride` is the
    /// distance between source row starts in **pixels, not bytes**, and must be
    /// at least `width`. Only `(height - 1) * source_stride + width` pixels are
    /// required: padding after the final row is optional. Padding and trailing
    /// pixels are ignored; a subslice can select a region of a larger source.
    ///
    /// All four channels replace the destination verbatim, even at zero alpha.
    /// There is no blending, premultiplication or color-space conversion; either
    /// straight or premultiplied input retains its original representation.
    /// Destination depth values and pixels outside the rectangle are unchanged.
    /// Safe borrowing requires separate source and destination storage.
    ///
    /// # Errors
    ///
    /// Validates nonzero dimensions, destination bounds, source stride, checked
    /// layout arithmetic and source length, in that order. Any error leaves the
    /// entire framebuffer unchanged, including when the final source row is short.
    ///
    /// ```
    /// use ferravex_graphics::Framebuffer;
    ///
    /// let mut source = Framebuffer::new(4, 3).unwrap();
    /// source.pixels_mut()[5] = [120, 60, 30, 128];
    /// let mut atlas = Framebuffer::new(6, 4).unwrap();
    /// // Select a 2x2 region at source (1, 1) without allocating a crop.
    /// atlas.blit_rgba(3, 2, 2, 2, &source.pixels()[5..], 4).unwrap();
    /// assert_eq!(atlas.pixel(3, 2), Some([120, 60, 30, 128]));
    /// let before = atlas.pixels().to_vec();
    /// assert!(atlas.blit_rgba(0, 0, 2, 2, &[[0; 4]; 3], 2).is_err());
    /// assert_eq!(atlas.pixels(), before);
    /// ```
    pub fn blit_rgba(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        source: &[[u8; 4]],
        source_stride: usize,
    ) -> Result<(), ImageRegionError> {
        self.validate_region(x, y, width, height)?;
        let width = width as usize;
        let height = height as usize;
        if source_stride < width {
            return Err(ImageRegionError::InvalidStride);
        }
        let required = (height - 1)
            .checked_mul(source_stride)
            .and_then(|offset| offset.checked_add(width))
            .ok_or(ImageRegionError::LayoutOverflow)?;
        if source.len() < required {
            return Err(ImageRegionError::SourceTooShort {
                required,
                actual: source.len(),
            });
        }

        let destination_stride = self.width() as usize;
        let destination_start = y as usize * destination_stride + x as usize;
        let destination = self.pixels_mut();
        for row in 0..height {
            // Bounds and the last-row end were validated before the first write.
            let src = row * source_stride;
            let dst = destination_start + row * destination_stride;
            destination[dst..dst + width].copy_from_slice(&source[src..src + width]);
        }
        Ok(())
    }

    fn validate_region(
        &self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), ImageRegionError> {
        if width == 0 || height == 0 {
            return Err(ImageRegionError::InvalidDimensions);
        }
        // Subtraction after checking the origins avoids overflowing x + width/y + height.
        if x > self.width()
            || y > self.height()
            || width > self.width() - x
            || height > self.height() - y
        {
            return Err(ImageRegionError::OutOfBounds);
        }
        Ok(())
    }

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

    fn patterned_frame(width: u32, height: u32) -> Framebuffer {
        let mut frame = Framebuffer::new(width, height).unwrap();
        for (i, pixel) in frame.pixels_mut().iter_mut().enumerate() {
            *pixel = [i as u8, 255 - i as u8, i as u8 * 7, [0, 128, 255][i % 3]];
        }
        frame
    }

    fn rendered_frame() -> Framebuffer {
        use crate::{
            Camera, ClipDepth, Color, Mat4, Mesh, MeshCache, RenderItem, SoftwareRenderer,
        };
        use crate::{Vec3, Vertex, vec3};

        let mut renderer = SoftwareRenderer::new(8, 8).unwrap();
        let mut cache = MeshCache::new();
        let mesh = cache
            .insert(Mesh {
                vertices: [
                    vec3(-0.8, -0.8, 0.5),
                    vec3(0.8, -0.8, 0.5),
                    vec3(0.0, 0.8, 0.5),
                ]
                .map(|position| Vertex::new(position, Vec3::Z, Color::new(1.0, 0.0, 0.0, 1.0)))
                .to_vec(),
                indices: vec![0, 1, 2],
            })
            .unwrap();
        renderer
            .render(
                &cache,
                &[RenderItem::new(mesh, Mat4::IDENTITY)],
                Camera {
                    view_projection: Mat4::IDENTITY,
                    position: Vec3::ZERO,
                    clip_depth: ClipDepth::ZeroToOne,
                },
            )
            .unwrap();
        assert!(renderer.framebuffer.depths().iter().any(|d| d.is_finite()));
        renderer.framebuffer
    }

    #[test]
    fn crop_preserves_row_order_rgba_and_owns_its_pixels() {
        let frame = patterned_frame(4, 3);
        let before = frame.pixels().to_vec();
        let depths = frame.depths().to_vec();
        let mut cropped = frame.crop(1, 1, 2, 2).unwrap();
        assert_eq!((cropped.width(), cropped.height()), (2, 2));
        assert_eq!(
            cropped.pixels(),
            &[before[5], before[6], before[9], before[10]]
        );
        assert_eq!(cropped.depths(), &[f32::INFINITY; 4]);
        cropped.pixels_mut().fill([255; 4]);
        assert_eq!(frame.pixels(), before);
        assert_eq!(frame.depths(), depths);
        assert_eq!(frame.crop(0, 0, 4, 3).unwrap().pixels(), before);
        assert_eq!(frame.crop(3, 2, 1, 1).unwrap().pixels(), &[before[11]]);
    }

    #[test]
    fn crop_rejects_empty_outside_and_overflowing_rectangles() {
        let frame = patterned_frame(4, 3);
        let before = frame.pixels().to_vec();
        for (x, y, width, height, error) in [
            (0, 0, 0, 1, ImageRegionError::InvalidDimensions),
            (0, 0, 1, 0, ImageRegionError::InvalidDimensions),
            (4, 0, 1, 1, ImageRegionError::OutOfBounds),
            (0, 3, 1, 1, ImageRegionError::OutOfBounds),
            (3, 1, 2, 1, ImageRegionError::OutOfBounds),
            (1, 2, 1, 2, ImageRegionError::OutOfBounds),
            (u32::MAX, 0, 2, 1, ImageRegionError::OutOfBounds),
            (0, u32::MAX, 1, 2, ImageRegionError::OutOfBounds),
            (1, 0, u32::MAX, 1, ImageRegionError::OutOfBounds),
            (0, 1, 1, u32::MAX, ImageRegionError::OutOfBounds),
        ] {
            assert_eq!(frame.crop(x, y, width, height).err(), Some(error));
            assert_eq!(frame.pixels(), before);
        }
    }

    #[test]
    fn blit_skips_padding_and_copies_all_alpha_values_verbatim() {
        let mut frame = patterned_frame(4, 3);
        let before = frame.pixels().to_vec();
        let transparent = [201, 102, 53, 0];
        let translucent = [120, 60, 30, 128];
        let opaque = [10, 20, 30, 255];
        let padding = [99; 4];
        // No padding is required after the final row.
        let source = [
            transparent,
            translucent,
            padding,
            padding,
            opaque,
            transparent,
        ];
        frame.blit_rgba(2, 1, 2, 2, &source, 4).unwrap();
        let mut expected = before;
        expected[6..8].copy_from_slice(&source[..2]);
        expected[10..12].copy_from_slice(&source[4..6]);
        assert_eq!(frame.pixels(), expected);

        // Extra source pixels, including final-row padding, are ignored.
        let mut padded_source = source.to_vec();
        padded_source.extend([padding; 3]);
        frame.blit_rgba(2, 1, 2, 2, &padded_source, 4).unwrap();
        assert_eq!(frame.pixels(), expected);
    }

    #[test]
    fn region_operations_leave_rendered_depth_unchanged() {
        let mut frame = rendered_frame();
        let pixels = frame.pixels().to_vec();
        let depths = frame.depths().to_vec();
        let cropped = frame.crop(0, 0, 8, 8).unwrap();
        assert_eq!(cropped.pixels(), pixels);
        assert!(cropped.depths().iter().all(|&d| d == f32::INFINITY));
        assert_eq!(frame.depths(), depths);

        let replacement = [[111, 22, 3, 0]; 64];
        frame.blit_rgba(0, 0, 8, 8, &replacement, 8).unwrap();
        assert_eq!(frame.pixels(), replacement);
        assert_eq!(frame.depths(), depths);
    }

    #[test]
    fn blit_rejects_bad_bounds_stride_and_overflow_before_any_write() {
        let mut frame = rendered_frame();
        let pixels = frame.pixels().to_vec();
        let depths = frame.depths().to_vec();
        let source = [[42; 4]; 64];
        for (x, y, width, height, stride, error) in [
            (0, 0, 0, 1, 0, ImageRegionError::InvalidDimensions),
            (0, 0, 1, 0, 0, ImageRegionError::InvalidDimensions),
            (8, 0, 1, 1, 1, ImageRegionError::OutOfBounds),
            (0, 8, 1, 1, 1, ImageRegionError::OutOfBounds),
            (7, 0, 2, 1, 2, ImageRegionError::OutOfBounds),
            (0, 7, 1, 2, 1, ImageRegionError::OutOfBounds),
            (u32::MAX, 0, 2, 1, 2, ImageRegionError::OutOfBounds),
            (0, u32::MAX, 1, 2, 1, ImageRegionError::OutOfBounds),
            (1, 0, u32::MAX, 1, 2, ImageRegionError::OutOfBounds),
            (0, 1, 1, u32::MAX, 1, ImageRegionError::OutOfBounds),
            (0, 0, 2, 2, 1, ImageRegionError::InvalidStride),
            (0, 0, 1, 1, 0, ImageRegionError::InvalidStride),
            (0, 0, 1, 2, usize::MAX, ImageRegionError::LayoutOverflow),
            (
                0,
                0,
                1,
                3,
                usize::MAX / 2 + 1,
                ImageRegionError::LayoutOverflow,
            ),
        ] {
            assert_eq!(
                frame.blit_rgba(x, y, width, height, &source, stride),
                Err(error)
            );
            assert_eq!(frame.pixels(), pixels);
            assert_eq!(frame.depths(), depths);
        }
    }

    #[test]
    fn blit_rejects_every_truncation_including_a_short_final_row_transactionally() {
        let mut frame = rendered_frame();
        let pixels = frame.pixels().to_vec();
        let depths = frame.depths().to_vec();
        let source = [[10, 20, 30, 0]; 5];
        for end in 0..source.len() {
            assert_eq!(
                frame.blit_rgba(2, 1, 2, 2, &source[..end], 3),
                Err(ImageRegionError::SourceTooShort {
                    required: 5,
                    actual: end,
                })
            );
            assert_eq!(frame.pixels(), pixels);
            assert_eq!(frame.depths(), depths);
        }
        frame.blit_rgba(2, 1, 2, 2, &source, 3).unwrap();
        assert_eq!(frame.pixel(3, 2), Some(source[4]));
        assert_eq!(frame.depths(), depths);
    }

    #[test]
    fn single_row_blit_does_not_require_or_step_over_unused_stride() {
        let mut frame = patterned_frame(4, 3);
        let mut expected = frame.pixels().to_vec();
        let source = [[90, 80, 70, 60], [50, 40, 30, 20]];
        frame.blit_rgba(2, 2, 2, 1, &source, usize::MAX).unwrap();
        expected[10..12].copy_from_slice(&source);
        assert_eq!(frame.pixels(), expected);
    }

    #[test]
    fn all_small_regions_match_per_pixel_crop_and_blit_reference() {
        let source = patterned_frame(4, 3);
        let background = [213, 127, 29, 65];
        for sy in 0..source.height() {
            for sx in 0..source.width() {
                for height in 1..=source.height() - sy {
                    for width in 1..=source.width() - sx {
                        let cropped = source.crop(sx, sy, width, height).unwrap();
                        for y in 0..height {
                            for x in 0..width {
                                assert_eq!(cropped.pixel(x, y), source.pixel(sx + x, sy + y));
                            }
                        }
                        for dy in 0..=4 - height {
                            for dx in 0..=5 - width {
                                let mut destination = Framebuffer::new(5, 4).unwrap();
                                destination.pixels_mut().fill(background);
                                let offset = (sy * source.width() + sx) as usize;
                                destination
                                    .blit_rgba(
                                        dx,
                                        dy,
                                        width,
                                        height,
                                        &source.pixels()[offset..],
                                        source.width() as usize,
                                    )
                                    .unwrap();
                                for y in 0..destination.height() {
                                    for x in 0..destination.width() {
                                        let expected = if x >= dx
                                            && x < dx + width
                                            && y >= dy
                                            && y < dy + height
                                        {
                                            source.pixel(sx + x - dx, sy + y - dy)
                                        } else {
                                            Some(background)
                                        };
                                        assert_eq!(destination.pixel(x, y), expected);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn checksum_known_vectors() {
        assert_eq!(crc32(b"123456789".iter().copied()), 0xcbf43926);
        assert_eq!(adler32(b"Wikipedia"), 0x11e60398);
    }
}
