# Known Issues

- Using `cargo clippy` will fail if `clippy` wasn't installed by `rustup`. This situation is difficult to detect
because there is an existing `clippy` install on the Github runners that fails to function.
- It looks as if cargo-install build artifacts are dependent on the index, even though it does not appear that the index change affected any relied-upon crates.
