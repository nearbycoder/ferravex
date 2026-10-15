#[cfg(not(target_arch = "wasm32"))]
fn main() {
    emberline::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
