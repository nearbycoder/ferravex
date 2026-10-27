#[cfg(not(target_arch = "wasm32"))]
fn main() {
    picnic_panic::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
