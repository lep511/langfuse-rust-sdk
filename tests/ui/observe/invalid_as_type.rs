use langfuse_rust_sdk::observe;

#[observe(as_type = "llm")]
fn call() {}

fn main() {}
