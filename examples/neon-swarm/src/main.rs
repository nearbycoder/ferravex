#[cfg(not(target_arch = "wasm32"))]
fn main() {
    neon_swarm::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
