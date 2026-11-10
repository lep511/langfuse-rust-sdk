use langfuse_rust_sdk::observe;

#[observe]
const fn answer() -> u32 {
    42
}

fn main() {}
