# langfuse-rust-sdk-macros

The `#[observe]` attribute of [`langfuse-rust-sdk`](https://crates.io/crates/langfuse-rust-sdk).
Do not depend on this crate directly: enable the `macros` feature of `langfuse-rust-sdk` and use
`langfuse_rust_sdk::observe`, whose documentation describes the attribute.

```toml
[dependencies]
langfuse-rust-sdk = { version = "0.10", features = ["macros"] }
```
