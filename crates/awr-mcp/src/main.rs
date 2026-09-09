use clap::Parser;
use rmcp::{ServiceExt, transport::stdio};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "awr-mcp",
    version,
    about = "Serve eight AWR tools over MCP stdio for one project directory"
)]
struct Args {
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args = Args::parse();
    let server = match awr_mcp::AwrServer::open(&args.project) {
        Ok(server) => server,
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::to_string(&error.report()).expect("error report serializes")
            );
            return std::process::ExitCode::FAILURE;
        }
    };
    // stdout belongs exclusively to MCP. Startup/transport diagnostics go to stderr.
    match server.serve(stdio()).await {
        Ok(service) => match service.waiting().await {
            Ok(_) => std::process::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("MCP transport: {error}");
                std::process::ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("MCP initialization: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
