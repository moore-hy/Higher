//! Web Search / Web Open（DEV-0052 / PHASE K-L §94-104, PHASE M 来源）。
//!
//! Brave Search API（X-Subscription-Token；Key 只在 Rust backend，绝不进前端 log/TRAE_RUN/Prompt）。
//! web_open：只开 web_search 返回过或用户明确提供的 URL；SSRF 全拒（file/localhost/私网/链路本地/metadata）；
//! 重定向后复查；单页下载 ≤5MB、纯文本 ≤1MB；HTML 正文提取（script/style/nav 剥离；scraper 被 SAC 拦→手写）。

use serde_json::Value as J;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WebSource {
    pub sid: String, // S1 / S2 ...（Source Registry）
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub published_at: Option<String>,
    pub source_type: String,
    pub retrieved_at: String,
}

/// §96-98 Brave Search。错误人话映射；Key 不回显。
pub async fn brave_search(
    api_key: &str,
    query: &str,
    count: u32,
    freshness: Option<&str>,
) -> Result<Vec<(String, String, String, Option<String>)>, String> {
    if api_key.trim().is_empty() {
        return Err("尚未配置 Brave Search API Key（设置 → 联网搜索）".to_string());
    }
    let n = count.clamp(1, 10);
    let mut url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
        urlencode(query),
        n
    );
    if let Some(f) = freshness {
        url.push_str(&format!("&freshness={}", urlencode(f)));
    }
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = http
        .get(&url)
        .header("X-Subscription-Token", api_key)
        .header("Accept", "application/json")
        .header("Accept-Encoding", "gzip")
        .send()
        .await
        .map_err(|e| if e.is_timeout() { "搜索请求超时".to_string() } else { "网络请求失败".to_string() })?;
    let status = resp.status();
    let text = resp.text().await.map_err(|_| "读取搜索响应失败".to_string())?;
    match status.as_u16() {
        401 | 403 => return Err("Brave API Key 无效或未授权".to_string()),
        429 => return Err("Brave 搜索请求过多（限流），请稍后重试".to_string()),
        422 => return Err("搜索参数无效".to_string()),
        s if s >= 500 => return Err("Brave 搜索服务暂不可用".to_string()),
        _ => {}
    }
    let v: J = serde_json::from_str(&text).map_err(|_| "Brave 响应格式异常（无法解析 JSON）".to_string())?;
    let mut out = Vec::new();
    if let Some(results) = v.get("web").and_then(|w| w.get("results")).and_then(|r| r.as_array()) {
        for r in results {
            let title = r.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let u = r.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let snippet = r.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let age = r.get("age").and_then(|x| x.as_str()).map(|s| s.to_string());
            if !u.is_empty() {
                out.push((title, u, snippet, age));
            }
        }
    }
    Ok(out)
}

/// §101-102 SSRF Guard（含字符串与 host 检查）。
pub fn ssrf_check(url_str: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(url_str).map_err(|_| format!("URL 无效：{}", truncate(url_str, 60)))?;
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("只允许 http/https URL（file:// 等被拒绝）".to_string());
    }
    let host = url.host_str().unwrap_or("").to_lowercase();
    if host.is_empty() {
        return Err("URL 缺少主机名".to_string());
    }
    let blocked_hosts = [
        "localhost", "127.0.0.1", "0.0.0.0", "::1", "[::1]", "ip6-localhost",
        "metadata.google.internal", "169.254.169.254",
    ];
    if blocked_hosts.iter().any(|h| host == *h || host.ends_with(&format!(".{}", h))) {
        return Err("禁止访问本机 / 内部服务地址".to_string());
    }
    // 私有 IPv4 段（直接 IP 访问）
    if let Some(ip) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        if ip == "::1" || ip.starts_with("fe80:") || ip.starts_with("fc") || ip.starts_with("fd") {
            return Err("禁止访问本机 / 私有 IPv6 地址".to_string());
        }
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() == 4 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
        let oct: Vec<u32> = parts.iter().filter_map(|p| p.parse().ok()).collect();
        if oct.len() == 4 {
            let (a, b) = (oct[0], oct[1]);
            let priv4 = a == 10
                || a == 127
                || a == 0
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 168)
                || (a == 169 && b == 254)
                || (a == 100 && (64..=127).contains(&b));
            if priv4 {
                return Err("禁止访问私有 / 链路本地网络地址".to_string());
            }
        }
    }
    Ok(url)
}

/// §102 重定向后复查：最终 URL 再跑 ssrf_check。
pub fn redirect_check(final_url: &str) -> Result<(), String> {
    ssrf_check(final_url).map(|_| ())
}

/// §103-104 web_open：下载 ≤5MB → 提取正文文本 ≤1MB。
pub async fn web_open(url_str: &str) -> Result<String, String> {
    let url = ssrf_check(url_str)?;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::custom(|att| {
            // 每次重定向做 SSRF 复查
            if ssrf_check(att.url().as_str()).is_err() {
                att.error("redirect to blocked address")
            } else if att.previous().len() > 5 {
                att.stop()
            } else {
                att.follow()
            }
        }))
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = http
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Higher/1.0")
        .send()
        .await
        .map_err(|e| if e.is_timeout() { "网页请求超时".to_string() } else { format!("网页请求失败：{}", short_err(&e)) })?;
    let final_url = resp.url().clone();
    redirect_check(final_url.as_str())?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("网页返回 HTTP {}", status.as_u16()));
    }
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    // 限额读取（5MB；reqwest 0.12 无 bytes_with_limit → 手动 chunk 循环）
    let mut limited: Vec<u8> = Vec::new();
    let cap = 5 * 1024 * 1024;
    loop {
        match resp.chunk().await {
            Ok(Some(c)) => {
                limited.extend_from_slice(&c);
                if limited.len() > cap {
                    return Err("网页内容过大（超过 5MB 限制）".to_string());
                }
            }
            Ok(None) => break,
            Err(_) => return Err("下载网页中断".to_string()),
        }
    }
    if !content_type.contains("html") && !content_type.contains("text") {
        return Err(format!("不支持的网页内容类型：{}", truncate(&content_type, 40)));
    }
    let body = String::from_utf8_lossy(&limited);
    let text = extract_html_text(&body);
    let text = if text.is_empty() { body.to_string() } else { text };
    let cut: String = text.chars().take(1024 * 1024).collect();
    Ok(cut)
}

/// §104：HTML → 正文文本（script/style/head/noscript/整块剥离 + 标签剥离）。
/// scraper 依赖链被 SAC 拦 → 手写等价提取。
pub fn extract_html_text(html: &str) -> String {
    let lower = html.to_lowercase();
    let mut out = String::with_capacity(html.len() / 2);
    // 逐块扫描移除 script/style 等整块
    let mut i = 0usize;
    let b = html.as_bytes();
    let noise = ["script", "style", "noscript", "head", "svg"];
    'outer: while i < b.len() {
        if b[i] == b'<' && i + 1 < b.len() && (b[i + 1] as char).is_ascii_alphabetic() {
            // 找标签名
            let mut j = i + 1;
            let mut name = String::new();
            while j < b.len() && name.len() < 12 && (b[j] as char).is_ascii_alphabetic() {
                name.push(b[j] as char);
                j += 1;
            }
            let ln = name.to_lowercase();
            if noise.contains(&ln.as_str()) {
                // 找 </name>
                let close = format!("</{}>", ln);
                if let Some(p) = lower[i..].find(&close) {
                    i = i + p + close.len();
                    continue 'outer;
                }
            }
            // 其他标签：跳到 '>'
            let mut k = j;
            let mut in_q: Option<u8> = None;
            while k < b.len() {
                match in_q {
                    Some(q) => {
                        if b[k] == q {
                            in_q = None;
                        }
                    }
                    None => {
                        if b[k] == b'"' || b[k] == b'\'' {
                            in_q = Some(b[k]);
                        } else if b[k] == b'>' {
                            break;
                        }
                    }
                }
                k += 1;
            }
            // 块级标签换行
            if matches!(ln.as_str(), "p" | "div" | "br" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "section" | "article") {
                out.push('\n');
            }
            i = (k + 1).min(b.len());
            continue;
        }
        if b[i] == b'<' {
            // 注释/闭合/doctype：跳到 '>'
            let mut k = i;
            while k < b.len() && b[k] != b'>' {
                k += 1;
            }
            i = (k + 1).min(b.len());
            continue;
        }
        let l = utf8_len(b[i]);
        let e = (i + l).min(b.len());
        out.push_str(&String::from_utf8_lossy(&b[i..e]));
        i = e;
    }
    // 实体 + 空白规整
    let s = html_entities(&out)
        .replace("\r\n", "\n")
        .replace('\t', " ");
    let mut cleaned = String::with_capacity(s.len());
    let mut last_nl = false;
    let mut last_sp = false;
    for ch in s.chars() {
        match ch {
            '\n' => {
                if !last_nl {
                    cleaned.push('\n');
                }
                last_nl = true;
                last_sp = false;
            }
            ' ' => {
                if !last_sp && !last_nl {
                    cleaned.push(' ');
                }
                last_sp = true;
            }
            c => {
                cleaned.push(c);
                last_nl = false;
                last_sp = false;
            }
        }
    }
    cleaned.trim().to_string()
}

fn html_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn short_err(e: &reqwest::Error) -> String {
    if e.is_connect() {
        "无法连接".to_string()
    } else {
        "网络错误".to_string()
    }
}
