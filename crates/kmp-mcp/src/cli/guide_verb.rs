use kmp_mcp::guide;

pub(super) async fn run_guide_command(arguments: &[&str]) -> i32 {
    match guide::sync(arguments).await {
        Ok(message) => {
            println!("{message}");
            0
        }
        Err(error) => {
            eprintln!("kmp-mcp guide: {error}");
            2
        }
    }
}
