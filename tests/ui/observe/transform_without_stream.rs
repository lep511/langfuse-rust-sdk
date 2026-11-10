use langfuse_rust_sdk::observe;

fn join(items: &[serde_json::Value]) -> String {
    items.len().to_string()
}

#[observe(transform_to_string = join)]
fn call() -> Vec<String> {
    Vec::new()
}

fn main() {}
