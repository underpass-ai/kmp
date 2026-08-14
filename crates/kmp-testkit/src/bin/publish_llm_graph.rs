use std::env;
use std::error::Error;
use std::fs;

use kmp_testkit::{llm_graph_to_projection_events, parse_llm_graph_batch};

struct Args {
    input: String,
    nats_url: String,
    subject_prefix: String,
    run_id: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = parse_args(env::args().skip(1))?;
    let payload = fs::read_to_string(&args.input)?;
    let batch = parse_llm_graph_batch(&payload)?;
    let messages = llm_graph_to_projection_events(&batch, &args.subject_prefix, &args.run_id)?;

    let client = async_nats::connect(&args.nats_url).await?;
    for (subject, payload) in &messages {
        client
            .publish(subject.clone(), payload.clone().into())
            .await?;
    }
    client.flush().await?;

    println!(
        "published {} projection messages for root {} to {}",
        messages.len(),
        batch.root_node_id,
        args.nats_url
    );

    Ok(())
}

fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, Box<dyn Error + Send + Sync>> {
    let mut input = None;
    let mut nats_url = None;
    let mut subject_prefix = "rehydration".to_string();
    let mut run_id = "llm-graph".to_string();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = args.next(),
            "--nats-url" => nats_url = args.next(),
            "--subject-prefix" => {
                subject_prefix = args.next().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "--subject-prefix requires a value",
                    )
                })?
            }
            "--run-id" => {
                run_id = args.next().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "--run-id requires a value",
                    )
                })?
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("unknown argument `{other}`"),
                )));
            }
        }
    }

    let input = input.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "--input is required")
    })?;
    let nats_url = nats_url.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "--nats-url is required")
    })?;

    Ok(Args {
        input,
        nats_url,
        subject_prefix,
        run_id,
    })
}

fn print_usage() {
    eprintln!(
        "usage: publish_llm_graph --input <path> --nats-url <url> [--subject-prefix <prefix>] [--run-id <id>]"
    );
}
