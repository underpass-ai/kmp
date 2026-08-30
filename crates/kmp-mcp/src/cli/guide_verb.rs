use kmp_mcp::guide::{GuideSyncReceiptMapper, NativeGuide};

pub(super) async fn run_guide_command(arguments: &[&str]) -> i32 {
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
