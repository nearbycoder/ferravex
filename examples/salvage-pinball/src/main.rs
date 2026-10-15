#[cfg(not(target_arch = "wasm32"))]
fn main() {
    salvage_pinball::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
