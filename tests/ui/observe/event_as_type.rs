use langfuse_rust_sdk::observe;

#[observe(as_type = "event")]
fn happened() {}

fn main() {}
