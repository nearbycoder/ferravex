#[cfg(not(target_arch = "wasm32"))]
fn main() {
    physics_3d::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
