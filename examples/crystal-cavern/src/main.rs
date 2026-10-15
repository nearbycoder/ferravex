#[cfg(not(target_arch = "wasm32"))]
fn main() {
    crystal_cavern::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
