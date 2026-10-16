use super::PcmBlock;
use std::sync::atomic::{AtomicBool, AtomicUsize};
pub(super) struct Device;
impl Device {
    pub(super) fn open(_: &str) -> Result<Self, i32> {
        Err(-1300)
    }
    pub(super) fn write(
        &mut self,
        _: &PcmBlock,
        _: &AtomicBool,
        _: &AtomicUsize,
    ) -> Result<usize, i32> {
        Err(-1300)
    }
}
