#[cfg(not(target_arch = "wasm32"))]
fn main() {
    marble_run::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
