use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerRequest {
    Ping {
        id: String,
    },
    Capabilities {
        id: String,
    },
    DownloadModel {
        id: String,
        model_dir: PathBuf,
    },
    Generate {
        id: String,
        text: String,
        voice: String,
        speed: f32,
        output_path: PathBuf,
        model_dir: PathBuf,
    },
    Shutdown {
        id: String,
    },
}
impl WorkerRequest {
    fn id(&self) -> &str {
        match self {
            Self::Ping { id }
            | Self::Capabilities { id }
            | Self::DownloadModel { id, .. }
            | Self::Generate { id, .. }
            | Self::Shutdown { id } => id,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkerResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub response_type: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub payload: Value,
}

pub struct WorkerSupervisor {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    request_gate: Mutex<()>,
}

impl WorkerSupervisor {
    pub async fn spawn(
        program: impl AsRef<Path>,
        args: &[String],
        worker_root: impl AsRef<Path>,
    ) -> Result<Self> {
        let root = worker_root.as_ref();
        let mut child = Command::new(program.as_ref())
            .args(args)
            .current_dir(root)
            .env("PYTHONPATH", root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| {
                format!("starting Kokoro worker with {}", program.as_ref().display())
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("worker stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("worker stdout unavailable"))?;
        Ok(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            request_gate: Mutex::new(()),
        })
    }

    pub async fn ping(&self) -> Result<WorkerResponse> {
        self.request(WorkerRequest::Ping {
            id: uuid::Uuid::new_v4().to_string(),
        })
        .await
    }

    pub async fn shutdown(&self) -> Result<()> {
        let request = WorkerRequest::Shutdown {
            id: uuid::Uuid::new_v4().to_string(),
        };
        let _ = self.request(request).await;
        let mut child = self.child.lock().await;
        if child.try_wait()?.is_none() {
            child.kill().await?;
        }
        Ok(())
    }

    pub async fn request(&self, request: WorkerRequest) -> Result<WorkerResponse> {
        let _gate = self.request_gate.lock().await;
        let request_id = request.id().to_owned();
        let mut encoded = serde_json::to_vec(&request)?;
        encoded.push(b'\n');
        {
            let mut stdin = self.stdin.lock().await;
            stdin
                .write_all(&encoded)
                .await
                .context("writing worker request")?;
            stdin.flush().await?;
        }
        let mut stdout = self.stdout.lock().await;
        loop {
            let mut line = String::new();
            let read = stdout
                .read_line(&mut line)
                .await
                .context("reading worker response")?;
            if read == 0 {
                bail!("Kokoro worker exited unexpectedly");
            }
            let response: WorkerResponse =
                serde_json::from_str(line.trim()).context("decoding worker response")?;
            if response.id != request_id {
                continue;
            }
            match response.response_type.as_str() {
                "progress" => continue,
                "error" => bail!(
                    response
                        .message
                        .unwrap_or_else(|| "Kokoro worker failed".into())
                ),
                "ready" | "complete" => return Ok(response),
                other => bail!("unexpected worker response type: {other}"),
            }
        }
    }
}
