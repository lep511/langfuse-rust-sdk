use langfuse_rust_sdk::observe;

#[observe(skip(token))]
fn call(api_key: &str) -> usize {
    api_key.len()
}

fn main() {}
