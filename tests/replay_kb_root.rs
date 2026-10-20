//! Frozen-oracle replay for `kb-root` (fixtures recorded once from the retired Python original).
mod common;

#[test]
fn replays_frozen_python_oracle() {
    common::replay("kb_root", env!("CARGO_BIN_EXE_kb-root"));
}
