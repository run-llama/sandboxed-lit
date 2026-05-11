use std::io::Write;
use std::sync::Arc;

use agent_sdk::{
    AgentEvent, CancellationToken, DynamicToolName, ThreadId, Tool, ToolContext, ToolRegistry,
    ToolResult, ToolTier, builder, providers::OpenAIResponsesProvider,
};
use microsandbox::Sandbox;
use serde_json::{Value, json};

use crate::sandbox::{create_or_get_sandbox, list_files, read_file, run_bash_command};

// ANSI color codes
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const ITALIC: &str = "\x1b[3m";

const CYAN: &str = "\x1b[36m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const GRAY: &str = "\x1b[90m";

/// A tool that lists all the available files in the sandbox
struct ListFiles;

/// A tool that reads files available in the sandbox
struct ReadFile;

/// A tool to execute bash commands
struct Bash;

impl Tool<Arc<Sandbox>> for ListFiles {
    type Name = DynamicToolName;

    fn name(&self) -> DynamicToolName {
        DynamicToolName::new("list_files")
    }

    // Optional: human-readable display name for UIs
    fn display_name(&self) -> &'static str {
        "List Files"
    }

    fn description(&self) -> &'static str {
        "List all the available files in the working directory"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn tier(&self) -> ToolTier {
        // Observe = no confirmation needed
        // Confirm = requires user approval (triggers yield/resume)
        ToolTier::Observe
    }

    async fn execute(
        &self,
        ctx: &ToolContext<Arc<Sandbox>>,
        _input: Value,
    ) -> anyhow::Result<ToolResult> {
        match list_files(ctx.app.clone(), None).await {
            Ok(f) => Ok(ToolResult::success(f.join(", "))),
            Err(e) => Ok(ToolResult::error(e.to_string())),
        }
    }
}

impl Tool<Arc<Sandbox>> for ReadFile {
    type Name = DynamicToolName;

    fn name(&self) -> DynamicToolName {
        DynamicToolName::new("read_file")
    }

    // Optional: human-readable display name for UIs
    fn display_name(&self) -> &'static str {
        "Read File"
    }

    fn description(&self) -> &'static str {
        "Read a file by providing its absolute path. Uses liteparse to parse the file content if the file is a PDF, an image or an Office document, otherwise reads the file text content."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path of the file, e.g. /app/data/file.pdf"
                }
            },
            "required": ["file_path"]
        })
    }

    fn tier(&self) -> ToolTier {
        ToolTier::Observe
    }

    async fn execute(
        &self,
        ctx: &ToolContext<Arc<Sandbox>>,
        input: Value,
    ) -> anyhow::Result<ToolResult> {
        let file_path = input["file_path"].as_str().unwrap_or("").to_string();
        match read_file(ctx.app.clone(), file_path).await {
            Ok(f) => Ok(ToolResult::success(f)),
            Err(e) => Ok(ToolResult::error(e.to_string())),
        }
    }
}

impl Tool<Arc<Sandbox>> for Bash {
    type Name = DynamicToolName;

    fn name(&self) -> DynamicToolName {
        DynamicToolName::new("bash")
    }

    // Optional: human-readable display name for UIs
    fn display_name(&self) -> &'static str {
        "Bash"
    }

    fn description(&self) -> &'static str {
        "Execute a bash command by providing the main command (e.g. 'ls') and an array of arguments (e.g. ['./tests', '-la'])"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The main command to execute, e.g. 'ls'"
                },
                "args": {
                    "type": "array",
                    "description": "Array of arguments provided to the main command, e.g. ['./tests', '-la']",
                    "items": {
                        "type": "string"
                    }
                }
            },
            "required": ["command", "args"]
        })
    }

    fn tier(&self) -> ToolTier {
        ToolTier::Observe
    }

    async fn execute(
        &self,
        ctx: &ToolContext<Arc<Sandbox>>,
        input: Value,
    ) -> anyhow::Result<ToolResult> {
        let default: &Vec<Value> = &vec![];
        let args: &Vec<Value> = input["args"].as_array().unwrap_or(default);
        let collected: Vec<String> = args
            .iter()
            .map(|e| {
                e.as_str()
                    .map(String::from)
                    .unwrap_or_else(|| e.to_string())
            })
            .collect();
        let command = input["command"].as_str().unwrap_or("").to_string();
        match run_bash_command(ctx.app.clone(), command, &collected).await {
            Ok(f) => Ok(ToolResult::success(f)),
            Err(e) => Ok(ToolResult::error(e.to_string())),
        }
    }
}

pub async fn run_agent(prompt: String, mount: Option<String>) -> anyhow::Result<()> {
    let sandbox = create_or_get_sandbox(mount).await?;
    let tool_ctx = ToolContext::new(sandbox);
    let mut tools = ToolRegistry::new();
    tools.register(ListFiles);
    tools.register(ReadFile);
    tools.register(Bash);

    let api_key = std::env::var("OPENAI_API_KEY")?;

    let agent = builder::<Arc<Sandbox>>()
        .provider(OpenAIResponsesProvider::new(api_key, "gpt-4.1".to_string()))
        .tools(tools)
        .build();

    let thread_id = ThreadId::new();
    let cancel = CancellationToken::new();

    let (mut events, _) = agent.run(
        thread_id,
        agent_sdk::AgentInput::Text(prompt),
        tool_ctx,
        cancel.clone(),
    );

    while let Some(envelope) = events.recv().await {
        match envelope.event {
            AgentEvent::Text {
                message_id: _,
                text,
            } => {
                print!("{text}");
                let _ = std::io::stdout().flush();
            }
            AgentEvent::Thinking {
                message_id: _,
                text,
            } => {
                println!("{DIM}{ITALIC}💭 {text}{RESET}");
            }
            AgentEvent::Done { .. } => break,
            AgentEvent::Error { message, .. } => {
                eprintln!("{BOLD}{RED}✗ Error:{RESET} {RED}{message}{RESET}");
            }
            AgentEvent::ToolCallStart {
                id,
                name: _,
                display_name,
                input,
                tier: _,
            } => {
                println!("\n{CYAN}{BOLD}⚙ {display_name}{RESET} {GRAY}[{id}]{RESET}");
                println!(
                    "{GRAY}{DIM}{}{RESET}",
                    serde_json::to_string_pretty(&input).unwrap_or("{}".to_string())
                );
            }
            AgentEvent::ToolCallEnd {
                id: _,
                name: _,
                display_name,
                result,
            } => {
                let label = display_name;
                if result.success {
                    println!(
                        "{GREEN}✓ {label}{RESET} {GRAY}{DIM}{}{RESET}",
                        result.output
                    );
                } else {
                    println!("{RED}✗ {label}{RESET} {RED}{DIM}{}{RESET}", result.output);
                }
            }
            _ => {}
        }
    }

    Ok(())
}
