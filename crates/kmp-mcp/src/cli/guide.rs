//! `kmp-mcp guide sync` — regenerate the installed guide assets.

use kmp_mcp::guide::{GuideSyncReceiptMapper, NativeGuide};

pub(super) async fn run(arguments: &[&str]) -> i32 {
    match NativeGuide::execute(arguments).await {
        Ok(receipt) => {
            println!("{}", GuideSyncReceiptMapper::to_text(&receipt));
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp guide: {error}");
            2
        }
    }
}
