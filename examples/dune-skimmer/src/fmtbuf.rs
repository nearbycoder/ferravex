//! Allocation-free formatting for per-frame HUD strings.
//!
//! Steady-state frames format the clock, speed, labels and hints into fixed
//! stack buffers instead of `String`s, so drawing the HUD allocates nothing.
use std::fmt::{self, Write};

/// A fixed-capacity UTF-8 buffer. Writes past capacity are truncated at a
/// character boundary rather than failing.
#[derive(Clone, Copy)]
pub(super) struct Buf<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Buf<N> {
    pub const fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }
    pub fn as_str(&self) -> &str {
        // Only whole UTF-8 sequences are ever copied in.
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> Write for Buf<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let room = N - self.len;
        let mut take = s.len().min(room);
        while !s.is_char_boundary(take) {
            take -= 1;
        }
        self.bytes[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

impl<const N: usize> std::ops::Deref for Buf<N> {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

/// `format!` into a 96-byte stack buffer.
macro_rules! buf {
    ($($arg:tt)*) => {{
        let mut out = $crate::fmtbuf::Buf::<96>::new();
        let _ = ::std::fmt::Write::write_fmt(&mut out, format_args!($($arg)*));
        out
    }};
}
pub(super) use buf;

/// `mm:ss.cc`, carrying rounding into the next minute.
pub(super) fn clock(time: f32) -> Buf<16> {
    let centiseconds = (time.max(0.) * 100.).round() as u64;
    let mut out = Buf::new();
    let _ = write!(
        out,
        "{:02}:{:02}.{:02}",
        centiseconds / 6000,
        centiseconds / 100 % 60,
        centiseconds % 100
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_truncate_on_char_boundaries_and_clock_matches_time_label() {
        let mut b = Buf::<5>::new();
        let _ = write!(b, "ab×cd");
        assert_eq!(b.as_str(), "ab×c");
        let mut b = Buf::<3>::new();
        let _ = write!(b, "ab×");
        assert_eq!(b.as_str(), "ab"); // '×' is two bytes and never split.
        assert_eq!(&*buf!("{} / {}", 3, 14), "3 / 14");
        for t in [0., 59.999, 62.345, 119.999] {
            assert_eq!(&*clock(t), crate::time_label(t));
        }
    }
}
