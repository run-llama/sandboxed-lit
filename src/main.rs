mod agent;
mod sandbox;

use clap::Parser;

/// Sandboxed Lit: run an LLM agent inside a microsandbox with file/bash tools.
#[derive(Parser, Debug)]
#[command(name = "sandboxed-lit", version, about)]
struct Cli {
    /// Prompt to send to the agent.
    #[arg(short, long)]
    prompt: String,

    /// Optional host directory to mount into the sandbox at /app/data.
    /// Defaults to the current directory when omitted.
    #[arg(short, long, value_name = "PATH", default_value = None)]
    volume: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    agent::run_agent(cli.prompt, cli.volume).await
}
