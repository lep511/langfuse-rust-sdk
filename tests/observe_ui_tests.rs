//! Compile errors of `#[observe]` (feature `macros`); expected output in
//! `tests/ui/observe/*.stderr` (regenerate with `TRYBUILD=overwrite`).
#![cfg(feature = "macros")]

#[test]
fn invalid_uses_fail_to_compile() {
    trybuild::TestCases::new().compile_fail("tests/ui/observe/*.rs");
}
