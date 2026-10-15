#[cfg(not(target_arch = "wasm32"))]
fn main() {
    pocket_rally::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
