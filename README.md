# sandboxed-lit

A small Rust CLI that runs an LLM agent inside a [microsandbox](https://microsandbox.dev) VM. The agent uses OpenAI's GPT models via [`agent-sdk`](https://crates.io/crates/agent-sdk) and has tools to list files, read files (parsing PDFs / images / Office docs through [`liteparse`](https://github.com/run-llama/liteparse)), and run bash commands, all confined to the sandbox.

## How it works

- **`src/sandbox.rs`** — Creates (or reuses) a microsandbox named `lit-sandbox` from the `ghcr.io/run-llama/liteparse:main` image with 2 CPUs and 1 GB of RAM, working dir `/app/`, and a bind mount at `/app/data`. Exposes:
  - `create_or_get_sandbox(volume)` — boots / attaches to the sandbox.
  - `list_files(sandbox, dir)` — recursively lists files under `/app/data`.
  - `read_file(sandbox, path)` — reads a file; routes PDFs, images and Office docs through `lit parse` for structured extraction.
  - `run_bash_command(sandbox, cmd, args)` — runs an arbitrary command inside the sandbox and returns `{stdout, stderr}`.
- **`src/agent.rs`** — Wraps those functions as three `agent-sdk` tools (`list_files`, `read_file`, `bash`), registers them, builds an OpenAI-backed agent, streams events to the terminal with colored output, and runs until completion.
- **`src/main.rs`** — A `clap` CLI that parses the prompt and optional mount path and calls `agent::run_agent`.

## Requirements

- Rust (edition 2024)
- A running microsandbox host (see the [microsandbox docs](https://github.com/microsandbox/microsandbox))
- An `OPENAI_API_KEY` environment variable

## Build

```sh
cargo build --release
```

## Usage

```sh
sandboxed-lit --prompt "<your prompt>" [--volume <host-path>]
```

Options:

| Flag | Short | Description |
| --- | --- | --- |
| `--prompt` | `-p` | Prompt to send to the agent (required). |
| `--volume` | `-v` | Host directory to mount at `/app/data` inside the sandbox. Defaults to the current directory. |

### Examples

Run with the current directory mounted:

```sh
export ANTHROPIC_API_KEY=sk-ant-...
sandboxed-lit -p "Summarize every PDF in the working directory."
```

Mount a specific folder:

```sh
sandboxed-lit \
  -p "List the files, then read report.pdf and extract the key findings." \
  -v /Users/me/documents
```

Files in the mounted directory are visible to the agent at `/app/data/...`.
