#[cfg(not(target_arch = "wasm32"))]
fn main() {
    glowtide::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
