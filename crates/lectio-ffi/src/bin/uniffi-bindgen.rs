//! Generates the Swift (and Kotlin) bindings for `lectio_ffi` from the
//! compiled library: `cargo run -p lectio-ffi --bin uniffi-bindgen --
//! generate --library <lib> --language swift --out-dir <dir>`.

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    uniffi::uniffi_bindgen_main();
}
