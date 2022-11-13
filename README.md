# GitHub actions for Rust written in Rust
[![CI](https://github.com/FrancisRussell/github-rust-actions/workflows/CI/badge.svg)](https://github.com/FrancisRussell/github-rust-actions/actions?query=workflow%3ACI)

GitHub action for Rust, written in Rust and compiled to WebAssembly.

## About

[actions-rs](https://github.com/actions-rs), the de-facto default for
Rust-related GitHub actions appears to be all but abandoned. This repository is
an experiement in replacing those actions with ones written in Rust, but
compiled down to WebAssembly. This should make them both portable across
platforms and more easily maintainable by developers who only know Rust.

## Status

Not yet usable in any form as an action but basic Rustup download and execution
functionality has been implemented.

## Acknowledements

This repository is based off the template created by Peter Evans
([@peter-evans](https://github.com/peter-evans))
[here](https://github.com/peter-evans/rust-wasm-action).

## License

[MIT](LICENSE)
