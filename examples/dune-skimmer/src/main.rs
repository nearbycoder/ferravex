#[cfg(not(target_arch = "wasm32"))]
fn main() {
    dune_skimmer::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
