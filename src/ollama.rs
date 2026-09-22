use std::time::Duration;

use anyhow::Context;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

use crate::config::normalize_host;

#[derive(Clone, Debug)]
pub struct Installed {
    pub name: String,
    pub size: u64,
    pub family: String,
    pub parameter_size: String,
    pub quantization: String,
    pub modified: String,
}

#[derive(Clone, Debug)]
pub struct RunningModel {
    pub name: String,
    pub size: u64,
    pub expires: String,
}

#[derive(Clone, Debug, Default)]
pub struct ModelDetail {
    pub family: String,
    pub parameter_size: String,
    pub quantization: String,
    pub context_length: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ChatTurn {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct ChatDone {
    pub tokens: u64,
    pub tokens_per_sec: f64,
}

pub enum ChatEvent {
    Delta { content: String, thinking: String },
    Done(ChatDone),
}

pub struct PullTick {
    pub status: String,
    pub completed: u64,
    pub total: u64,
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

fn root(host: &str) -> String {
    normalize_host(host)
}

pub async fn tags(host: &str) -> anyhow::Result<Vec<Installed>> {
    let url = format!("{}/api/tags", root(host));
    let response = http()
        .get(url)
        .timeout(Duration::from_secs(4))
        .send()
        .await?
        .error_for_status()?;
    let body: Value = response.json().await?;
    let mut models = Vec::new();
    for item in body["models"].as_array().into_iter().flatten() {
        let details = &item["details"];
        models.push(Installed {
            name: item["name"].as_str().unwrap_or("").to_string(),
            size: item["size"].as_u64().unwrap_or(0),
            family: details["family"].as_str().unwrap_or("").to_string(),
            parameter_size: details["parameter_size"].as_str().unwrap_or("").to_string(),
            quantization: details["quantization_level"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            modified: item["modified_at"].as_str().unwrap_or("").to_string(),
        });
    }
    Ok(models)
}

pub async fn running(host: &str) -> anyhow::Result<Vec<RunningModel>> {
    let url = format!("{}/api/ps", root(host));
    let response = http()
        .get(url)
        .timeout(Duration::from_secs(4))
        .send()
        .await?
        .error_for_status()?;
    let body: Value = response.json().await?;
    let mut models = Vec::new();
    for item in body["models"].as_array().into_iter().flatten() {
        models.push(RunningModel {
            name: item["name"].as_str().unwrap_or("").to_string(),
            size: item["size"].as_u64().unwrap_or(0),
            expires: item["expires_at"].as_str().unwrap_or("").to_string(),
        });
    }
    Ok(models)
}

pub async fn show(host: &str, model: &str) -> anyhow::Result<ModelDetail> {
    let url = format!("{}/api/show", root(host));
    let response = http()
        .post(url)
        .json(&json!({ "model": model }))
        .timeout(Duration::from_secs(8))
        .send()
        .await?
        .error_for_status()?;
    let body: Value = response.json().await?;
    let details = &body["details"];
    let mut context = None;
    if let Some(info) = body["model_info"].as_object() {
        for (key, value) in info {
            if key.ends_with("context_length") {
                if let Some(n) = value.as_u64() {
                    context = Some(context.map_or(n, |c: u64| c.max(n)));
                }
            }
        }
    }
    Ok(ModelDetail {
        family: details["family"].as_str().unwrap_or("").to_string(),
        parameter_size: details["parameter_size"].as_str().unwrap_or("").to_string(),
        quantization: details["quantization_level"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        context_length: context,
    })
}

pub async fn delete_model(host: &str, model: &str) -> anyhow::Result<()> {
    let url = format!("{}/api/delete", root(host));
    let response = http()
        .delete(url)
        .json(&json!({ "model": model }))
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("{status} {body}");
    }
    Ok(())
}

/// Ask Ollama to drop the model. An empty generate with keep_alive 0 unloads it.
pub async fn unload(host: &str, model: &str) -> anyhow::Result<()> {
    let url = format!("{}/api/generate", root(host));
    let response = http()
        .post(url)
        .json(&json!({ "model": model, "keep_alive": 0 }))
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("{status} {body}");
    }
    Ok(())
}

#[derive(Serialize)]
struct ChatBody<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    options: ChatOptions,
    keep_alive: &'a str,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatOptions {
    temperature: f32,
    top_p: f32,
    num_ctx: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
}

pub struct ChatParams {
    pub temperature: f32,
    pub top_p: f32,
    pub num_ctx: u32,
    pub seed: Option<i64>,
    pub keep_alive: String,
}

pub async fn stream_chat(
    host: &str,
    model: &str,
    system: &str,
    turns: &[ChatTurn],
    params: &ChatParams,
    tx: &UnboundedSender<ChatEvent>,
) -> anyhow::Result<()> {
    let mut messages = Vec::new();
    if !system.trim().is_empty() {
        messages.push(ChatMessage {
            role: "system".into(),
            content: system.to_string(),
        });
    }
    for turn in turns {
        messages.push(ChatMessage {
            role: turn.role.clone(),
            content: turn.content.clone(),
        });
    }
    let body = ChatBody {
        model,
        messages: &messages,
        stream: true,
        options: ChatOptions {
            temperature: params.temperature,
            top_p: params.top_p,
            num_ctx: params.num_ctx,
            seed: params.seed,
        },
        keep_alive: &params.keep_alive,
    };
    let url = format!("{}/api/chat", root(host));
    let response = http().post(url).json(&body).send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("{status} {text}");
    }
    read_ndjson(response, |value| {
        if value["done"].as_bool().unwrap_or(false) {
            let tokens = value["eval_count"].as_u64().unwrap_or(0);
            let nanos = value["eval_duration"].as_u64().unwrap_or(0);
            let tokens_per_sec = if nanos == 0 {
                0.0
            } else {
                tokens as f64 / (nanos as f64 / 1e9)
            };
            let _ = tx.send(ChatEvent::Done(ChatDone {
                tokens,
                tokens_per_sec,
            }));
        } else {
            let message = &value["message"];
            let _ = tx.send(ChatEvent::Delta {
                content: message["content"].as_str().unwrap_or("").to_string(),
                thinking: message["thinking"].as_str().unwrap_or("").to_string(),
            });
        }
    })
    .await
}

pub async fn stream_pull(
    host: &str,
    model: &str,
    insecure: bool,
    tx: &UnboundedSender<PullTick>,
) -> anyhow::Result<()> {
    let url = format!("{}/api/pull", root(host));
    let mut payload = json!({ "model": model, "stream": true });
    if insecure {
        payload["insecure"] = json!(true);
    }
    let response = http().post(url).json(&payload).send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("{status} {text}");
    }
    read_ndjson(response, |value| {
        let _ = tx.send(PullTick {
            status: value["status"].as_str().unwrap_or("").to_string(),
            completed: value["completed"].as_u64().unwrap_or(0),
            total: value["total"].as_u64().unwrap_or(0),
        });
    })
    .await
}

async fn read_ndjson(
    response: reqwest::Response,
    mut on_value: impl FnMut(Value),
) -> anyhow::Result<()> {
    let mut stream = response.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("reading response")?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().trim_end_matches('\r').to_string();
            buf.drain(..=pos);
            if line.is_empty() {
                continue;
            }
            let value: Value =
                serde_json::from_str(&line).with_context(|| format!("bad json: {line}"))?;
            if let Some(error) = value["error"].as_str() {
                anyhow::bail!("{error}");
            }
            on_value(value);
        }
    }
    Ok(())
}

pub fn service_state() -> String {
    if systemd_unit_loaded() {
        if let Ok(output) = std::process::Command::new("systemctl")
            .args(["is-active", "ollama"])
            .output()
        {
            let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !state.is_empty() {
                return state;
            }
        }
    }
    if serve_pids().is_empty() {
        "inactive".into()
    } else {
        "running".into()
    }
}

/// Start, stop, or restart Ollama. Uses the systemd unit when it is installed.
/// `sudo` is used only after a permission failure, and it inherits the terminal
/// so the password prompt is visible. Without a unit, this manages `ollama serve`.
pub fn control(action: &str) -> Result<String, String> {
    if !matches!(action, "start" | "stop" | "restart") {
        return Err(format!("unknown service action {action}"));
    }
    let result = if systemd_unit_loaded() {
        control_systemd(action)
    } else {
        control_user_process(action)
    };
    if result.is_ok() && matches!(action, "start" | "restart") {
        std::thread::sleep(Duration::from_millis(600));
    }
    result.map(|_| match action {
        "start" => "Started Ollama".into(),
        "stop" => "Stopped Ollama".into(),
        _ => "Restarted Ollama".into(),
    })
}

fn control_systemd(action: &str) -> Result<(), String> {
    match systemctl(action, false) {
        Ok(()) => Ok(()),
        Err(message) if needs_privilege(&message) => {
            println!("Administrator permission is required to {action} the Ollama service.");
            systemctl(action, true)
        }
        Err(message) => Err(message),
    }
}

fn systemctl(action: &str, sudo: bool) -> Result<(), String> {
    let output = if sudo {
        std::process::Command::new("sudo")
            .args(["systemctl", action, "ollama"])
            .status()
            .map_err(|err| err.to_string())?
    } else {
        let output = std::process::Command::new("systemctl")
            .args([action, "ollama"])
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|err| err.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        let mut detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if detail.is_empty() {
            detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
        if detail.is_empty() {
            detail = format!("systemctl {action} ollama failed");
        }
        return Err(detail);
    };
    if output.success() {
        Ok(())
    } else {
        Err(format!("sudo systemctl {action} ollama failed"))
    }
}

fn needs_privilege(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("authentication")
        || lower.contains("access denied")
        || lower.contains("permission denied")
        || lower.contains("not authorized")
        || lower.contains("polkit")
        || lower.contains("interactive")
}

fn systemd_unit_loaded() -> bool {
    std::process::Command::new("systemctl")
        .args(["show", "-p", "LoadState", "--value", "ollama"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "loaded")
        .unwrap_or(false)
}

fn control_user_process(action: &str) -> Result<(), String> {
    match action {
        "stop" => stop_user_server(),
        "restart" => {
            let _ = stop_user_server();
            start_user_server()
        }
        _ => start_user_server(),
    }
}

fn start_user_server() -> Result<(), String> {
    if !serve_pids().is_empty() {
        return Ok(());
    }
    let log_path = crate::store::data_dir().join("ollama.log");
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| err.to_string())?;
    let err_log = log.try_clone().map_err(|err| err.to_string())?;
    use std::os::unix::process::CommandExt;
    std::process::Command::new("ollama")
        .arg("serve")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log))
        .stderr(std::process::Stdio::from(err_log))
        .process_group(0)
        .spawn()
        .map_err(|err| format!("could not start ollama serve: {err}"))?;
    Ok(())
}

fn stop_user_server() -> Result<(), String> {
    let pids = serve_pids();
    if pids.is_empty() {
        return Ok(());
    }
    for pid in pids {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
    }
    Ok(())
}

fn serve_pids() -> Vec<u32> {
    let Ok(output) = std::process::Command::new("ps")
        .args(["-eo", "pid,args"])
        .output()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (pid, args) = line.split_once(' ')?;
            let pid = pid.trim().parse().ok()?;
            if args.contains("ollama") && args.split_whitespace().any(|part| part == "serve") {
                Some(pid)
            } else {
                None
            }
        })
        .collect()
}

pub fn redirect_failure(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("redirect") || lower.contains("realm") || lower.contains("blocked")
}

/// The raw Ollama error quotes the entire CDN URL, which does not fit in the status line.
pub fn short_pull_error(message: &str) -> String {
    if redirect_failure(message) {
        "Hugging Face redirected the download to its CDN and Ollama blocked it.".into()
    } else if message.len() > 160 {
        format!("{}…", message.chars().take(160).collect::<String>())
    } else {
        message.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privilege_errors_ask_for_sudo() {
        assert!(super::needs_privilege(
            "Interactive authentication required."
        ));
        assert!(super::needs_privilege("Access denied"));
        assert!(!super::needs_privilege("Unit ollama.service not found."));
    }

    #[test]
    fn spots_huggingface_redirect_errors() {
        assert!(redirect_failure("realm host huggingface.co does not match"));
        assert!(redirect_failure("blocked redirect to a different host"));
        assert!(!redirect_failure("model not found"));
        let short = short_pull_error(
            "Head \"https://us.aws.cdn.hf.co/very-long\", blocked redirect to a different host",
        );
        assert_eq!(
            short,
            "Hugging Face redirected the download to its CDN and Ollama blocked it."
        );
    }
}
