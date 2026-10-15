#[cfg(not(target_arch = "wasm32"))]
fn main() {
    siege_foundry::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
