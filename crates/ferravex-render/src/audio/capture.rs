//! Optional bounded PCM16 WAV capture, without intermediate sample allocations.
use std::{
    fs::File,
    io::{self, Seek, SeekFrom, Write},
    path::Path,
};
pub struct Capture {
    file: File,
    frames: u32,
    failed: bool,
}
impl Capture {
    pub fn new(path: &Path) -> io::Result<Self> {
        let mut file = File::create(path)?;
        file.write_all(&ferravex_play::audio::encode_wav(48000, &[]).unwrap())?;
        Ok(Self {
            file,
            frames: 0,
            failed: false,
        })
    }
    pub fn write(&mut self, frames: &[[f32; 2]]) {
        if self.failed {
            return;
        }
        let mut bytes = [0_u8; 4096];
        let remaining = ferravex_play::audio::MAX_CLIP_FRAMES.saturating_sub(self.frames as usize);
        for chunk in frames[..frames.len().min(remaining)].chunks(1024) {
            for (sample, out) in chunk.iter().flatten().zip(bytes.chunks_exact_mut(2)) {
                let v = if sample.is_finite() {
                    sample.clamp(-1., 1.)
                } else {
                    0.
                };
                let scale = if v < 0. { 32768. } else { 32767. };
                out.copy_from_slice(&((v * scale).round() as i16).to_le_bytes());
            }
            if self.file.write_all(&bytes[..chunk.len() * 4]).is_err() {
                self.failed = true;
                return;
            }
            self.frames += chunk.len() as u32;
        }
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        let len = self.frames * 4;
        let _ = (|| -> io::Result<()> {
            self.file.seek(SeekFrom::Start(4))?;
            self.file.write_all(&(36 + len).to_le_bytes())?;
            self.file.seek(SeekFrom::Start(40))?;
            self.file.write_all(&len.to_le_bytes())?;
            self.file.flush()
        })();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_patches_the_header_and_bounds_nonfinite_samples() {
        let path = std::env::temp_dir().join(format!("ferravex-audio-{}.wav", std::process::id()));
        let mut capture = Capture::new(&path).unwrap();
        capture.write(&[[0.5, -0.5], [f32::NAN, 2.]]);
        drop(capture);
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        let clip = ferravex_play::audio::Clip::wav(&bytes).unwrap();
        assert_eq!(clip.samples.len(), 2);
        assert_eq!(clip.samples[1], [0., 32767. / 32768.]);
    }
}
