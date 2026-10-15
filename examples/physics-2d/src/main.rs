#[cfg(not(target_arch = "wasm32"))]
fn main() {
    physics_2d::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
