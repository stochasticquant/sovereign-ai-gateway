/// Smoke test: verify that all crates compile and link correctly.
#[test]
fn workspace_compiles() {
    // If this test runs, the entire workspace compiled and linked successfully.
    // Reference types from each crate to ensure they're reachable.
    let _id = gateway_core::types::TenantId::new();
}
