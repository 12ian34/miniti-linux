//! Minimal Streamable-HTTP MCP client for Docs MCP search (BYOK Playbook).
//! Port of `../miniti-api/lib/mcpClient.ts` + `docsMcp.ts`: JSON-RPC over
//! POST, JSON or SSE responses, search-tool discovery, chunk normalization,
//! and the same SSRF guard (https only, no private / local hosts).

use std::net::IpAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const MCP_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_CHUNKS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocChunk {
    pub title: String,
    pub url: Option<String>,
    pub text: String,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchTool {
    pub name: String,
    pub query_argument: String,
}

fn is_blocked_ipv4(a: u8, b: u8) -> bool {
    a == 10
        || a == 127
        || a == 0
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 100 && (64..=127).contains(&b))
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            is_blocked_ipv4(o[0], o[1])
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                let o = v4.octets();
                return is_blocked_ipv4(o[0], o[1]);
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Validate a user-provided MCP URL: https only, no credentials, no local /
/// private / metadata hosts.
pub fn assert_safe_mcp_url(raw: &str) -> Result<url::Url, String> {
    let u =
        url::Url::parse(raw.trim()).map_err(|_| "docs_mcp_url must be a valid URL".to_string())?;
    if u.scheme() != "https" {
        return Err("docs_mcp_url must use https".into());
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err("docs_mcp_url must not include credentials".into());
    }
    let host = u.host_str().unwrap_or_default().to_lowercase();
    if host == "localhost"
        || host.ends_with(".localhost")
        || host == "metadata.google.internal"
        || host.ends_with(".internal")
        || host.ends_with(".local")
    {
        return Err("docs_mcp_url host is not allowed".into());
    }
    if let Ok(ip) = host
        .trim_matches(|c| c == '[' || c == ']')
        .parse::<IpAddr>()
    {
        if is_blocked_ip(ip) {
            return Err("docs_mcp_url host is not allowed".into());
        }
    }
    Ok(u)
}

fn first_string_property(schema: Option<&Value>) -> Option<String> {
    let props = schema?.get("properties")?.as_object()?;
    let is_string = |v: &Value| v.get("type").map(|t| t == "string").unwrap_or(true);
    if let Some(required) = schema?.get("required").and_then(|r| r.as_array()) {
        for key in required.iter().filter_map(|k| k.as_str()) {
            if let Some(p) = props.get(key) {
                if is_string(p) {
                    return Some(key.to_string());
                }
            }
        }
    }
    props
        .iter()
        .find(|(_, v)| is_string(v))
        .map(|(k, _)| k.clone())
}

/// Prefer a tool whose name matches "search" with a string query argument.
pub fn discover_search_tool(tools: &[McpTool]) -> Option<SearchTool> {
    const BLOCKED: &[&str] = &[
        "submit_feedback",
        "feedback",
        "create",
        "update",
        "delete",
        "write",
        "send",
        "post",
        "put",
        "patch",
    ];
    let mut best: Option<(i32, SearchTool)> = None;
    for t in tools {
        let lname = t.name.to_lowercase();
        if BLOCKED.iter().any(|b| lname.starts_with(b)) {
            continue;
        }
        let Some(arg) = first_string_property(t.input_schema.as_ref()) else {
            continue;
        };
        let mut score = 0;
        if lname.contains("search") {
            score += 100;
        }
        if arg == "query" || arg == "q" {
            score += 20;
        }
        if lname.contains("knowledge") || lname.contains("docs") || lname.contains("documentation")
        {
            score += 10;
        }
        if lname.contains("filesystem")
            || lname.contains("shell")
            || lname.contains("feedback")
            || lname.contains("submit")
        {
            score -= 50;
        }
        if best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
            best = Some((
                score,
                SearchTool {
                    name: t.name.clone(),
                    query_argument: arg,
                },
            ));
        }
    }
    best.filter(|(s, _)| *s >= 0).map(|(_, t)| t)
}

fn capture_after(text: &str, label: &str) -> Option<String> {
    for line in text.lines() {
        let l = line.trim_start();
        if l.len() >= label.len() && l[..label.len()].eq_ignore_ascii_case(label) {
            let v = l[label.len()..].trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect()
    }
}

/// Mintlify-style "Title: … / Link: … / Content: …" text, JSON blobs, or plain text.
fn parse_text_chunk(text: &str) -> Option<DocChunk> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let title = capture_after(trimmed, "Title:");
    let link = capture_after(trimmed, "Link:").filter(|l| l.starts_with("http"));
    let content = trimmed
        .find("Content:")
        .map(|i| trimmed[i + "Content:".len()..].trim().to_string());
    if title.is_some() || link.is_some() || content.is_some() {
        return Some(DocChunk {
            title: truncate(
                &title
                    .clone()
                    .or_else(|| link.clone())
                    .unwrap_or_else(|| "Documentation".into()),
                200,
            ),
            url: link,
            text: truncate(&content.unwrap_or_else(|| trimmed.to_string()), 4000),
            score: None,
        });
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            if let Some(c) = chunks_from_value(&v, 0).into_iter().next() {
                return Some(c);
            }
        }
    }
    Some(DocChunk {
        title: "Documentation".into(),
        url: None,
        text: truncate(trimmed, 4000),
        score: None,
    })
}

fn chunks_from_value(v: &Value, depth: usize) -> Vec<DocChunk> {
    if depth > 4 {
        return Vec::new();
    }
    match v {
        Value::String(s) => parse_text_chunk(s).into_iter().collect(),
        Value::Array(items) => items
            .iter()
            .flat_map(|i| chunks_from_value(i, depth + 1))
            .collect(),
        Value::Object(o) => {
            let get = |keys: &[&str]| {
                keys.iter()
                    .find_map(|k| o.get(*k).and_then(|x| x.as_str()).filter(|s| !s.is_empty()))
            };
            let title = get(&["title", "name"]);
            let url = get(&["url", "link", "href"]);
            let text = get(&["content", "text", "snippet", "markdown"]);
            if title.is_some() || url.is_some() || text.is_some() {
                return vec![DocChunk {
                    title: truncate(title.or(url).unwrap_or("Documentation"), 200),
                    url: url.map(String::from),
                    text: truncate(text.or(title).unwrap_or(""), 4000),
                    score: o.get("score").and_then(|s| s.as_f64()),
                }];
            }
            for key in ["results", "chunks", "documents", "items", "data"] {
                if let Some(nested) = o.get(key) {
                    let c = chunks_from_value(nested, depth + 1);
                    if !c.is_empty() {
                        return c;
                    }
                }
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// Normalize a `tools/call` result into deduped chunks (max 8).
pub fn normalize_tool_result(result: &Value) -> Vec<DocChunk> {
    if result
        .get("isError")
        .and_then(|b| b.as_bool())
        .unwrap_or(false)
    {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
        for item in content {
            if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                chunks.extend(chunks_from_value(&Value::String(t.to_string()), 0));
            } else {
                chunks.extend(chunks_from_value(item, 0));
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for c in chunks {
        if c.text.trim().is_empty() {
            continue;
        }
        let key = format!(
            "{}|{}|{}",
            c.url.clone().unwrap_or_default(),
            c.title,
            truncate(&c.text, 80)
        );
        if seen.insert(key) {
            out.push(c);
            if out.len() >= MAX_CHUNKS {
                break;
            }
        }
    }
    out
}

fn parse_jsonrpc_body(body: &str, content_type: &str) -> Result<Value, String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err("empty MCP response".into());
    }
    let is_sse = content_type.contains("text/event-stream")
        || trimmed.starts_with("event:")
        || trimmed.starts_with("data:");
    let parse_sse = |t: &str| -> Result<Value, String> {
        let last = t
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .next_back()
            .ok_or_else(|| "MCP SSE response missing data".to_string())?;
        serde_json::from_str(last.trim())
            .map_err(|_| "MCP SSE response is not valid JSON".to_string())
    };
    if is_sse {
        return parse_sse(trimmed);
    }
    serde_json::from_str(trimmed).or_else(|_| {
        if trimmed.contains("data:") {
            parse_sse(trimmed)
        } else {
            Err("MCP response is not valid JSON".to_string())
        }
    })
}

pub struct McpClient {
    url: String,
    http: reqwest::Client,
    session_id: Option<String>,
    initialized: bool,
}

impl McpClient {
    pub fn new(url: &str) -> Result<Self, String> {
        let u = assert_safe_mcp_url(url)?;
        let http = reqwest::Client::builder()
            .timeout(MCP_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            url: u.to_string(),
            http,
            session_id: None,
            initialized: false,
        })
    }

    async fn post(&mut self, payload: Value) -> Result<Option<Value>, String> {
        let mut req = self
            .http
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");
        if let Some(sid) = &self.session_id {
            req = req.header("Mcp-Session-Id", sid.clone());
        }
        let resp = req.json(&payload).send().await.map_err(|e| {
            if e.is_timeout() {
                "MCP request timed out".to_string()
            } else {
                format!("MCP request failed: {e}")
            }
        })?;
        if let Some(sid) = resp
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            self.session_id = Some(sid.to_string());
        }
        let status = resp.status();
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let body = resp.text().await.map_err(|e| e.to_string())?;
        if status == reqwest::StatusCode::ACCEPTED
            || (status.is_success() && body.trim().is_empty())
        {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(format!("MCP server returned HTTP {status}"));
        }
        let v = parse_jsonrpc_body(&body, &ct)?;
        if let Some(err) = v.get("error") {
            return Err(format!(
                "MCP error: {}",
                err.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown")
            ));
        }
        Ok(Some(v.get("result").cloned().unwrap_or(Value::Null)))
    }

    async fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value, String> {
        let id = chrono::Utc::now().timestamp_millis() % 1_000_000;
        let mut payload = json!({ "jsonrpc": "2.0", "id": id, "method": method });
        if let Some(p) = params {
            payload["params"] = p;
        }
        self.post(payload)
            .await?
            .ok_or_else(|| "MCP returned no result".to_string())
    }

    pub async fn initialize(&mut self) -> Result<(), String> {
        if self.initialized {
            return Ok(());
        }
        let r = self
            .request(
                "initialize",
                Some(json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "miniti", "version": crate::state::APP_VERSION }
                })),
            )
            .await?;
        if !r.is_object() {
            return Err("MCP initialize returned unexpected result".into());
        }
        let _ = self
            .post(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .await;
        self.initialized = true;
        Ok(())
    }

    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>, String> {
        self.initialize().await?;
        let r = self.request("tools/list", None).await?;
        let tools = r
            .get("tools")
            .and_then(|t| t.as_array())
            .ok_or_else(|| "MCP tools/list returned no tools".to_string())?;
        Ok(tools
            .iter()
            .filter_map(|t| serde_json::from_value::<McpTool>(t.clone()).ok())
            .filter(|t| !t.name.trim().is_empty())
            .collect())
    }

    pub async fn call_tool(&mut self, name: &str, args: Value) -> Result<Value, String> {
        self.initialize().await?;
        self.request(
            "tools/call",
            Some(json!({ "name": name, "arguments": args })),
        )
        .await
    }

    /// Discover the search tool and retrieve normalized chunks for `query`.
    pub async fn retrieve(&mut self, query: &str) -> Result<(SearchTool, Vec<DocChunk>), String> {
        let tools = self.list_tools().await?;
        let tool = discover_search_tool(&tools)
            .ok_or_else(|| "No searchable docs tool found on MCP server".to_string())?;
        let q: String = query.trim().chars().take(400).collect();
        let result = self
            .call_tool(&tool.name, json!({ tool.query_argument.clone(): q }))
            .await?;
        Ok((tool, normalize_tool_result(&result)))
    }
}

/// Settings "test connection": list tools and report the discovered search tool.
pub async fn probe(url: &str) -> Result<(String, Vec<String>), String> {
    let mut c = McpClient::new(url)?;
    let tools = c.list_tools().await?;
    let search = discover_search_tool(&tools)
        .ok_or_else(|| "No searchable docs tool found on MCP server".to_string())?;
    Ok((search.name, tools.into_iter().map(|t| t.name).collect()))
}

pub fn format_chunks_for_prompt(chunks: &[DocChunk]) -> String {
    chunks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "[{}] Title: {}\nURL: {}\n{}",
                i + 1,
                c.title,
                c.url.clone().unwrap_or_else(|| "null".into()),
                c.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_guard_matches_backend() {
        assert!(assert_safe_mcp_url("https://docs.lightdash.com/mcp").is_ok());
        for bad in [
            "http://docs.example.com/mcp",
            "https://localhost/mcp",
            "https://127.0.0.1/mcp",
            "https://10.1.2.3/mcp",
            "https://192.168.1.1/mcp",
            "https://169.254.169.254/latest",
            "https://metadata.google.internal/",
            "https://user:pw@docs.example.com/mcp",
            "https://[::1]/mcp",
            "https://[fd00::1]/mcp",
        ] {
            assert!(assert_safe_mcp_url(bad).is_err(), "{bad} should be blocked");
        }
    }

    #[test]
    fn discovers_search_tool_by_name_and_query_arg() {
        let tools: Vec<McpTool> = serde_json::from_value(json!([
            {"name": "submit_feedback", "inputSchema": {"properties": {"text": {"type": "string"}}}},
            {"name": "read_file", "inputSchema": {"properties": {"path": {"type": "string"}}}},
            {"name": "search_docs", "inputSchema": {"properties": {"query": {"type": "string"}}, "required": ["query"]}}
        ]))
        .unwrap();
        let t = discover_search_tool(&tools).unwrap();
        assert_eq!(t.name, "search_docs");
        assert_eq!(t.query_argument, "query");
        assert!(discover_search_tool(&[]).is_none());
    }

    #[test]
    fn normalizes_mintlify_text_and_json_chunks() {
        let result = json!({"content": [
            {"type": "text", "text": "Title: SSO\nLink: https://docs.example.com/sso\nContent: SAML is supported."},
            {"type": "text", "text": "{\"results\": [{\"title\": \"Retention\", \"url\": \"https://d/r\", \"content\": \"30 days\"}]}"},
            {"type": "text", "text": "Title: SSO\nLink: https://docs.example.com/sso\nContent: SAML is supported."}
        ]});
        let chunks = normalize_tool_result(&result);
        assert_eq!(chunks.len(), 2, "deduped");
        assert_eq!(chunks[0].title, "SSO");
        assert_eq!(
            chunks[0].url.as_deref(),
            Some("https://docs.example.com/sso")
        );
        assert_eq!(chunks[0].text, "SAML is supported.");
        assert_eq!(chunks[1].title, "Retention");
        assert!(normalize_tool_result(&json!({"isError": true, "content": []})).is_empty());
    }

    #[test]
    fn parses_json_and_sse_rpc_bodies() {
        let v = parse_jsonrpc_body(
            r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#,
            "application/json",
        )
        .unwrap();
        assert_eq!(v["result"]["ok"], true);
        let sse =
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}\n\n";
        let v = parse_jsonrpc_body(sse, "text/event-stream").unwrap();
        assert!(v["result"]["tools"].is_array());
        assert!(parse_jsonrpc_body("", "application/json").is_err());
    }
}
