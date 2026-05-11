use std::pin::Pin;
use std::sync::Arc;
use std::{path, vec};

use anyhow::anyhow;
use microsandbox::MicrosandboxError::{ExecFailed, ExecTimeout, SandboxNotFound};
use microsandbox::Sandbox;
use microsandbox::sandbox::{FsEntryKind, PullPolicy};
use serde_json::json;

const EXTENSIONS: &[&str] = &[
    "pdf", "jpg", "jpeg", "png", "gif", "bmp", "tiff", "webp", "svg", "doc", "docx", "docm", "odt",
    "rtf", "ppt", "pptx", "pptm", "odp", "xls", "xlsx", "xlsm", "ods", "csv", "tsv",
];

pub async fn create_or_get_sandbox(volume: Option<String>) -> anyhow::Result<Arc<Sandbox>> {
    let existing = Sandbox::get("lit-sandbox").await;
    match existing {
        Ok(b) => {
            let sb = b.start().await?;
            return Ok(Arc::new(sb));
        }
        Err(e) => match e {
            SandboxNotFound(_) => {}
            _ => return Err(anyhow!(e.to_string())),
        },
    }
    let new = Sandbox::builder("lit-sandbox")
        .image("ghcr.io/run-llama/liteparse:main")
        .cpus(2)
        .memory(1024)
        .workdir("/app/")
        .volume("/app/data", |m| m.bind(volume.unwrap_or(".".to_string())))
        .pull_policy(PullPolicy::IfMissing)
        .create()
        .await?;
    Ok(Arc::new(new))
}

pub fn list_files(
    sandbox: Arc<Sandbox>,
    dir: Option<String>,
) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<String>>> + Send>> {
    Box::pin(async move {
        let path = if let Some(d) = dir {
            format!("/app/data/{d}")
        } else {
            "/app/data".to_string()
        };
        let entries = sandbox.fs().list(&path).await?;
        let mut files = vec![];
        for entry in entries {
            match entry.kind {
                FsEntryKind::Directory => {
                    let mut children = list_files(sandbox.clone(), Some(entry.path)).await?;
                    files.append(&mut children);
                }
                _ => {
                    files.push(entry.path);
                }
            }
        }
        Ok(files)
    })
}

pub async fn read_file(sandbox: Arc<Sandbox>, file_path: String) -> anyhow::Result<String> {
    let p = path::PathBuf::from(&file_path);
    let ext = p.extension();
    if let Some(e) = ext
        && let Some(es) = e.to_str()
        && EXTENSIONS.contains(&es)
    {
        let output = sandbox.exec("lit", vec!["parse", &file_path]).await;
        match output {
            Ok(o) => return Ok(o.stdout()?),
            Err(e) => match e {
                ExecFailed(f) => return Ok(format!("Failed because of {}", f.message)),
                ExecTimeout(t) => {
                    return Ok(format!(
                        "Command did not execute within the timeout of {:?} ms",
                        t.as_millis()
                    ));
                }
                _ => return Err(anyhow!(e.to_string())),
            },
        }
    }
    let content = sandbox.fs().read_to_string(&file_path).await?;
    Ok(content)
}

pub async fn run_bash_command(
    sandbox: Arc<Sandbox>,
    command: String,
    args: &Vec<String>,
) -> anyhow::Result<String> {
    let output = sandbox.exec(&command, args).await;
    match output {
        Ok(o) => {
            let v = json!({ "stdout": o.stdout().unwrap_or_default(), "stderr": o.stderr().unwrap_or_default() });
            Ok(serde_json::to_string(&v)?)
        }
        Err(e) => match e {
            ExecFailed(f) => Ok(format!("Command execution failed because of {}", f.message)),
            ExecTimeout(t) => Ok(format!(
                "Command did not execute within the timeout of {:?} ms",
                t.as_millis()
            )),
            _ => Err(anyhow!(e.to_string())),
        },
    }
}
