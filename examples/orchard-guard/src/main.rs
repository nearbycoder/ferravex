#[cfg(not(target_arch = "wasm32"))]
fn main() {
    orchard_guard::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
