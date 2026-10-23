#[cfg(not(target_arch = "wasm32"))]
fn main() {
    mini_golf_3d::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
