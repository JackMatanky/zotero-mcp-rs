//! Command-line interface for the Zotero domain library (scaffold).

#[expect(
    clippy::print_stdout,
    reason = "primary output of a CLI tool is stdout"
)]
fn main() {
    let state = zotero_api::AppState::from_env();
    println!("zotero-cli: write access {}", state.is_write_enabled());
}
