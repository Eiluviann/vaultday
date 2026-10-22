//! `uniffi-bindgen-swift`, run by `scripts/build-xcframework.sh`.
//!
//! Living in the workspace pins the generator to the exact `uniffi` version
//! `crates/ffi` compiles its scaffolding with, so the Swift side can never
//! drift from the Rust side.

fn main() {
    uniffi::uniffi_bindgen_swift();
}
