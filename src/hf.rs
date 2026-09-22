use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;

use crate::util::{encode_form, encode_path_segment};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Downloads,
    Likes,
    Modified,
}

impl Sort {
    pub fn cycle(self) -> Self {
        match self {
            Self::Downloads => Self::Likes,
            Self::Likes => Self::Modified,
            Self::Modified => Self::Downloads,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Downloads => "downloads",
            Self::Likes => "likes",
            Self::Modified => "updated",
        }
    }

    fn api(self) -> &'static str {
        match self {
            Self::Downloads => "downloads",
            Self::Likes => "likes",
            Self::Modified => "lastModified",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Model {
    pub id: String,
    pub downloads: u64,
    pub likes: u64,
    pub last_modified: String,
}

#[derive(Clone, Debug)]
pub struct GgufFile {
    pub path: String,
    pub size: u64,
    pub quant: String,
}

pub fn search_url(query: &str, sort: Sort) -> String {
    format!(
        "https://huggingface.co/api/models?filter=gguf&sort={}&direction=-1&limit=30&search={}",
        sort.api(),
        encode_form(query)
    )
}

pub fn tree_url(repo: &str) -> String {
    let path = repo
        .split('/')
        .map(encode_path_segment)
        .collect::<Vec<_>>()
        .join("/");
    format!("https://huggingface.co/api/models/{path}/tree/main?recursive=true")
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .user_agent("ollatui")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub async fn search(query: &str, sort: Sort) -> anyhow::Result<Vec<Model>> {
    let response = http()
        .get(search_url(query, sort))
        .send()
        .await?
        .error_for_status()
        .context("huggingface search")?;
    let raw: Vec<ApiModel> = response.json().await?;
    Ok(raw
        .into_iter()
        .map(|m| Model {
            id: m.id,
            downloads: m.downloads,
            likes: m.likes,
            last_modified: m.last_modified.chars().take(10).collect(),
        })
        .collect())
}

pub async fn gguf_files(repo: &str) -> anyhow::Result<Vec<GgufFile>> {
    let response = http()
        .get(tree_url(repo))
        .send()
        .await?
        .error_for_status()
        .context("huggingface file list")?;
    let raw: Vec<TreeItem> = response.json().await?;
    let mut files: Vec<GgufFile> = raw
        .into_iter()
        .filter(|item| item.path.to_ascii_lowercase().ends_with(".gguf"))
        .map(|item| GgufFile {
            quant: quant_tag(&item.path),
            size: item.size,
            path: item.path,
        })
        .collect();
    files.sort_by(|a, b| {
        quant_rank(&a.quant)
            .cmp(&quant_rank(&b.quant))
            .then(b.size.cmp(&a.size))
    });
    Ok(files)
}

/// Ollama accepts the GGUF filename as the tag, which picks one file out of the repo.
pub fn ollama_ref(repo: &str, filename: &str) -> String {
    let file = filename.rsplit('/').next().unwrap_or(filename);
    format!("hf.co/{repo}:{file}")
}

pub fn quant_tag(filename: &str) -> String {
    let upper = filename.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    let mut best: Option<(usize, usize)> = None;
    let mut i = 0;
    while i < bytes.len() {
        if i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
            i += 1;
            continue;
        }
        if let Some(len) = match_quant(&upper[i..]) {
            best = Some((i, len));
            i += len;
        } else {
            i += 1;
        }
    }
    if let Some((start, len)) = best {
        return upper[start..start + len].to_string();
    }
    filename
        .rsplit('/')
        .next()
        .unwrap_or(filename)
        .trim_end_matches(".gguf")
        .trim_end_matches(".GGUF")
        .to_string()
}

fn match_quant(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    if s.starts_with("BF16") {
        return Some(4);
    }
    if s.starts_with("MXFP") {
        return take_prefixed(s, 4);
    }
    if b.first() == Some(&b'I') && b.get(1) == Some(&b'Q') {
        return take_q_body(s, 2);
    }
    if s.starts_with("F16") || s.starts_with("F32") || s.starts_with("F64") {
        return Some(3);
    }
    if b.first() == Some(&b'Q') {
        return take_q_body(s, 1);
    }
    None
}

fn take_prefixed(s: &str, prefix: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = prefix;
    let digits = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits {
        return None;
    }
    if i < b.len() && b[i] == b'_' {
        let seg = i + 1;
        let mut k = seg;
        while k < b.len() && b[k].is_ascii_alphanumeric() {
            k += 1;
        }
        if k > seg {
            i = k;
        }
    }
    Some(i)
}

fn take_q_body(s: &str, prefix: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = prefix;
    let digits = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits {
        return None;
    }
    for _ in 0..2 {
        if i < b.len() && b[i] == b'_' {
            let seg = i + 1;
            let mut k = seg;
            while k < b.len() && b[k].is_ascii_alphanumeric() {
                k += 1;
            }
            if k == seg {
                break;
            }
            i = k;
        }
    }
    Some(i)
}

fn quant_rank(tag: &str) -> u8 {
    match tag {
        "Q4_K_M" => 0,
        "Q5_K_M" => 1,
        "Q6_K" => 2,
        "Q8_0" => 3,
        "Q4_K_S" => 4,
        "Q5_K_S" => 5,
        "Q4_0" => 6,
        "Q3_K_M" => 7,
        "IQ4_XS" => 8,
        _ => 50,
    }
}

#[derive(Deserialize)]
struct ApiModel {
    id: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    likes: u64,
    #[serde(rename = "lastModified", default)]
    last_modified: String,
}

#[derive(Deserialize)]
struct TreeItem {
    path: String,
    #[serde(default)]
    size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_url_filters_gguf() {
        let url = search_url("qwen 3", Sort::Downloads);
        assert!(url.contains("filter=gguf"));
        assert!(url.contains("sort=downloads"));
        assert!(url.contains("direction=-1"));
        assert!(url.contains("limit=30"));
        assert!(url.contains("search=qwen+3"));
        let likes = search_url("llama", Sort::Likes);
        assert!(likes.contains("sort=likes"));
        let updated = search_url("llama", Sort::Modified);
        assert!(updated.contains("sort=lastModified"));
    }

    #[test]
    fn quant_tags_from_filenames() {
        assert_eq!(quant_tag("Llama-3.2-3B-Instruct-Q4_K_M.gguf"), "Q4_K_M");
        assert_eq!(quant_tag("model-IQ3_M.gguf"), "IQ3_M");
        assert_eq!(quant_tag("foo-BF16.gguf"), "BF16");
        assert_eq!(quant_tag("bar-IQ4_XS.gguf"), "IQ4_XS");
        assert_eq!(
            ollama_ref("unsloth/Qwen3-8B-GGUF", "Qwen3-8B-Q4_K_M.gguf"),
            "hf.co/unsloth/Qwen3-8B-GGUF:Qwen3-8B-Q4_K_M.gguf"
        );
    }
}
