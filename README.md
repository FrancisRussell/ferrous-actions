# GitHub actions for Rust written in Rust
[![CI](https://github.com/FrancisRussell/github-rust-actions/workflows/CI/badge.svg)](https://github.com/FrancisRussell/github-rust-actions/actions?query=workflow%3ACI)

GitHub action for Rust, written in Rust and compiled to WebAssembly.

## About

[actions-rs](https://github.com/actions-rs), the de-facto default for
Rust-related GitHub actions appears to be all but abandoned. This repository is
an experiment in replacing those actions with ones written in Rust, but
compiled down to WebAssembly. This should make them both portable across
platforms and more easily maintainable by developers who only know Rust.

## Status

- [ ] CI builds can produce commits in a release repository so that action can be used
- [x] `rustup` installation
- [x] `rustup` installation of specified toolchains
- [ ] `cross` support (implemented but untested)
- [x] Invoking `cargo` commands
- [x] Annotation generation from `cargo` commands
- [x] Validate that all known supplied parameters are used
- [ ] Handle conflicts between `cargo` and `rustup` installs of the same component.
- [x] Cache intermediate artifacts for `cargo install` invocations.
- [x] Include binary name in name of cached build artifacts.
- [x] Cache index
- [x] Cache downloaded crates
- [x] Cache downloaded Git repositories
- [x] Specify minimum time before items can be recached.
- [x] Drop unused indices from cache (keyed on Cargo.lock files)
- [x] Drop unused crates from cache (keyed on Cargo.lock files)
- [x] Drop unused Git repos from cache (keyed on Cargo.lock files)
- [ ] Drop unused `cargo install` build artifacts from cache
- [x] Ignore Rustup toolchain files for all package installs

## Acknowledgements

This repository is based off the template created by Peter Evans
([@peter-evans](https://github.com/peter-evans))
[here](https://github.com/peter-evans/rust-wasm-action).

## License

[MIT](LICENSE)
