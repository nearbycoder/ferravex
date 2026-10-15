#[cfg(not(target_arch = "wasm32"))]
fn main() {
    sky_courier::run();
}
#[cfg(target_arch = "wasm32")]
fn main() {}
