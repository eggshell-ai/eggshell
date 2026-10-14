use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::llm::{LlmResult, Tool};

const DEFAULT_CDP_PORT: u16 = 9222;

/// Finds an installed browser executable on Windows or fallback systems.
fn find_browser_executable() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        // Try Chrome first, then Edge
        let chrome = PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe");
        if chrome.is_file() {
            return Some(chrome);
        }
        let chrome_x86 = PathBuf::from(r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe");
        if chrome_x86.is_file() {
            return Some(chrome_x86);
        }
        let edge = PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe");
        if edge.is_file() {
            return Some(edge);
        }
        let edge_64 = PathBuf::from(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe");
        if edge_64.is_file() {
            return Some(edge_64);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let chrome = PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
        if chrome.is_file() {
            return Some(chrome);
        }
        let edge = PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge");
        if edge.is_file() {
            return Some(edge);
        }
    }

    #[cfg(target_os = "linux")]
    {
        for name in &["google-chrome", "google-chrome-stable", "chromium", "microsoft-edge"] {
            if let Ok(path) = which::which(name) {
                return Some(path);
            }
        }
    }

    None
}

/// A lightweight Chrome DevTools Protocol (CDP) client.
pub struct CdpSession {
    ws_url: String,
    next_id: AtomicU64,
}

impl CdpSession {
    pub async fn connect(port: u16) -> Result<Self, String> {
        let http_url = format!("http://127.0.0.1:{port}/json/version");
        let client = reqwest::Client::new();
        let res = client
            .get(&http_url)
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .map_err(|e| format!("Failed to reach CDP HTTP endpoint at {http_url}: {e}"))?;

        let body: Value = res
            .json()
            .await
            .map_err(|e| format!("Failed to parse CDP version response: {e}"))?;

        let ws_url = body
            .get("webSocketDebuggerUrl")
            .and_then(Value::as_str)
            .ok_or_else(|| "CDP endpoint did not return webSocketDebuggerUrl".to_string())?
            .to_string();

        Ok(Self {
            ws_url,
            next_id: AtomicU64::new(1),
        })
    }

    pub async fn send_command(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (ws_stream, _) = connect_async(&self.ws_url)
            .await
            .map_err(|e| format!("WebSocket connection failed to {}: {e}", self.ws_url))?;

        let (mut write, mut read) = ws_stream.split();

        let req = json!({
            "id": id,
            "method": method,
            "params": params
        });

        write
            .send(Message::Text(req.to_string().into()))
            .await
            .map_err(|e| format!("Failed to send CDP command: {e}"))?;

        let timeout = tokio::time::sleep(Duration::from_secs(10));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    return Err(format!("CDP command '{method}' timed out after 10s"));
                }
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(resp) = serde_json::from_str::<Value>(&text) {
                                if resp.get("id").and_then(Value::as_u64) == Some(id) {
                                    if let Some(err) = resp.get("error") {
                                        return Err(format!("CDP error: {}", err));
                                    }
                                    return Ok(resp.get("result").cloned().unwrap_or(Value::Null));
                                }
                            }
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => return Err(format!("WebSocket error while waiting for CDP response: {e}")),
                        None => return Err("WebSocket closed before CDP response arrived".to_string()),
                    }
                }
            }
        }
    }

    /// Evaluates javascript in the browser and returns the result value.
    pub async fn evaluate(&self, expression: &str) -> Result<Value, String> {
        let result = self
            .send_command(
                "Runtime.evaluate",
                json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": true
                }),
            )
            .await?;

        let val = result.get("result").and_then(|r| r.get("value")).cloned().unwrap_or(Value::Null);
        Ok(val)
    }
}

pub struct BrowserManager {
    child: Option<Child>,
    session: Option<CdpSession>,
    port: u16,
}

impl BrowserManager {
    pub fn new() -> Self {
        Self {
            child: None,
            session: None,
            port: DEFAULT_CDP_PORT,
        }
    }

    pub async fn ensure_browser(&mut self, url: Option<&str>) -> Result<&CdpSession, String> {
        // If session is already connected, test connection
        if let Some(ref session) = self.session {
            if session.evaluate("1 + 1").await.is_ok() {
                if let Some(u) = url {
                    let _ = session.send_command("Page.navigate", json!({ "url": u })).await;
                }
                return Ok(self.session.as_ref().unwrap());
            }
        }

        // Try connecting to an already running instance on port 9222
        if let Ok(session) = CdpSession::connect(self.port).await {
            if let Some(u) = url {
                let _ = session.send_command("Page.navigate", json!({ "url": u })).await;
            }
            self.session = Some(session);
            return Ok(self.session.as_ref().unwrap());
        }

        // Otherwise, spawn browser
        let browser_path = find_browser_executable()
            .ok_or_else(|| "No Chrome or Edge browser executable found on this system.".to_string())?;

        let user_data = std::env::temp_dir().join("eggshell_qa_browser_profile");
        let initial_url = url.unwrap_or("http://localhost:4000");

        let mut cmd = Command::new(&browser_path);
        cmd.args([
            &format!("--remote-debugging-port={}", self.port),
            &format!("--user-data-dir={}", user_data.display()),
            "--no-first-run",
            "--no-default-browser-check",
            initial_url,
        ]);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x00000010); // CREATE_NEW_CONSOLE
        }

        let child = cmd.spawn().map_err(|e| format!("Failed to spawn browser: {e}"))?;
        self.child = Some(child);

        // Wait up to 5 seconds for CDP endpoint to become ready
        let mut connected = false;
        for _ in 0..25 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            if let Ok(session) = CdpSession::connect(self.port).await {
                self.session = Some(session);
                connected = true;
                break;
            }
        }

        if !connected {
            return Err("Browser launched, but remote debugging port (9222) did not become available.".to_string());
        }

        Ok(self.session.as_ref().unwrap())
    }
}

// Global shared browser instance across QA tool calls
lazy_static::lazy_static! {
    static ref GLOBAL_BROWSER: Arc<Mutex<BrowserManager>> = Arc::new(Mutex::new(BrowserManager::new()));
}

// --- Browser Tools ---

/// Tool: browser_open
pub struct BrowserOpenTool {
    parameters: Map<String, Value>,
}

impl BrowserOpenTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to open (defaults to http://localhost:4000)."
                    }
                }
            })
            .as_object()
            .unwrap()
            .clone(),
        }
    }
}

impl Default for BrowserOpenTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BrowserOpenTool {
    fn name(&self) -> &str {
        "browser_open"
    }

    fn description(&self) -> &str {
        "Launches or connects to the local browser via Chrome DevTools Protocol (CDP) and navigates to the given URL."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let url = args.get("url").and_then(Value::as_str).unwrap_or("http://localhost:4000");
        let mut mgr = GLOBAL_BROWSER.lock().await;
        match mgr.ensure_browser(Some(url)).await {
            Ok(_) => Ok(json!({
                "success": true,
                "message": format!("Browser opened and navigated to {url}")
            })),
            Err(e) => Ok(json!({
                "success": false,
                "error": e
            })),
        }
    }
}

/// Tool: browser_navigate
pub struct BrowserNavigateTool {
    parameters: Map<String, Value>,
}

impl BrowserNavigateTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL or path to navigate to (e.g. 'http://localhost:4000/products' or '/products')."
                    }
                },
                "required": ["url"]
            })
            .as_object()
            .unwrap()
            .clone(),
        }
    }
}

impl Default for BrowserNavigateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BrowserNavigateTool {
    fn name(&self) -> &str {
        "browser_navigate"
    }

    fn description(&self) -> &str {
        "Navigates the open browser to a specified URL or route."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let raw_url = args
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| "url is required".to_string())?;

        let full_url = if raw_url.starts_with('/') {
            format!("http://localhost:4000{raw_url}")
        } else {
            raw_url.to_string()
        };

        let mut mgr = GLOBAL_BROWSER.lock().await;
        let session = match mgr.ensure_browser(Some(&full_url)).await {
            Ok(s) => s,
            Err(e) => return Ok(json!({ "success": false, "error": e })),
        };

        let _ = session.send_command("Page.navigate", json!({ "url": full_url })).await;
        // Small delay for initial page load
        tokio::time::sleep(Duration::from_millis(1500)).await;

        Ok(json!({
            "success": true,
            "message": format!("Navigated to {full_url}")
        }))
    }
}

/// Tool: browser_click
pub struct BrowserClickTool {
    parameters: Map<String, Value>,
}

impl BrowserClickTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector to click, or text label to search (e.g. 'button.ant-btn-primary', 'a[href=\"/products\"]', or text match)."
                    }
                },
                "required": ["selector"]
            })
            .as_object()
            .unwrap()
            .clone(),
        }
    }
}

impl Default for BrowserClickTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BrowserClickTool {
    fn name(&self) -> &str {
        "browser_click"
    }

    fn description(&self) -> &str {
        "Clicks an element on the currently active page matching a CSS selector or text."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let selector = args
            .get("selector")
            .and_then(Value::as_str)
            .ok_or_else(|| "selector is required".to_string())?;

        let mut mgr = GLOBAL_BROWSER.lock().await;
        let session = match mgr.ensure_browser(None).await {
            Ok(s) => s,
            Err(e) => return Ok(json!({ "success": false, "error": e })),
        };

        let js = format!(
            r#"(() => {{
                let sel = {sel};
                let el = document.querySelector(sel);
                if (!el) {{
                    // Try finding by text content
                    const all = Array.from(document.querySelectorAll('button, a, [role="button"], span, div'));
                    el = all.find(e => e.textContent && e.textContent.trim().toLowerCase() === sel.toLowerCase());
                }}
                if (el) {{
                    el.scrollIntoView({{ behavior: 'instant', block: 'center' }});
                    el.click();
                    return {{ found: true, tag: el.tagName, text: el.textContent ? el.textContent.trim().slice(0, 50) : '' }};
                }}
                return {{ found: false, error: 'Element not found for selector: ' + sel }};
            }})()"#,
            sel = serde_json::to_string(selector).unwrap_or_default()
        );

        match session.evaluate(&js).await {
            Ok(res) => {
                tokio::time::sleep(Duration::from_millis(500)).await;
                Ok(res)
            }
            Err(e) => Ok(json!({ "success": false, "error": e })),
        }
    }
}

/// Tool: browser_fill
pub struct BrowserFillTool {
    parameters: Map<String, Value>,
}

impl BrowserFillTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector of the input field (e.g. 'input[name=\"email\"]', '#username', 'input[type=\"password\"]')."
                    },
                    "value": {
                        "type": "string",
                        "description": "Value to type into the field."
                    }
                },
                "required": ["selector", "value"]
            })
            .as_object()
            .unwrap()
            .clone(),
        }
    }
}

impl Default for BrowserFillTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BrowserFillTool {
    fn name(&self) -> &str {
        "browser_fill"
    }

    fn description(&self) -> &str {
        "Fills an input or textarea element with a specified value and dispatches change/input events."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let selector = args.get("selector").and_then(Value::as_str).unwrap_or_default();
        let val = args.get("value").and_then(Value::as_str).unwrap_or_default();

        let mut mgr = GLOBAL_BROWSER.lock().await;
        let session = match mgr.ensure_browser(None).await {
            Ok(s) => s,
            Err(e) => return Ok(json!({ "success": false, "error": e })),
        };

        let js = format!(
            r#"(() => {{
                let sel = {sel};
                let val = {val};
                let el = document.querySelector(sel);
                if (!el) {{
                    return {{ found: false, error: 'Element not found: ' + sel }};
                }}
                el.focus();
                el.value = val;
                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return {{ found: true, valueSet: val }};
            }})()"#,
            sel = serde_json::to_string(selector).unwrap_or_default(),
            val = serde_json::to_string(val).unwrap_or_default()
        );

        match session.evaluate(&js).await {
            Ok(res) => Ok(res),
            Err(e) => Ok(json!({ "success": false, "error": e })),
        }
    }
}

/// Tool: browser_get_page_content
pub struct BrowserGetContentTool {
    parameters: Map<String, Value>,
}

impl BrowserGetContentTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "Optional CSS selector to inspect (e.g. '.ant-table', 'form', 'main'). Defaults to 'body'."
                    }
                }
            })
            .as_object()
            .unwrap()
            .clone(),
        }
    }
}

impl Default for BrowserGetContentTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BrowserGetContentTool {
    fn name(&self) -> &str {
        "browser_get_page_content"
    }

    fn description(&self) -> &str {
        "Inspects current page content: current URL, page title, visible text, links, forms, and tables."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, args: Value) -> LlmResult<Value> {
        let selector = args.get("selector").and_then(Value::as_str).unwrap_or("body");

        let mut mgr = GLOBAL_BROWSER.lock().await;
        let session = match mgr.ensure_browser(None).await {
            Ok(s) => s,
            Err(e) => return Ok(json!({ "success": false, "error": e })),
        };

        let js = format!(
            r#"(() => {{
                let root = document.querySelector({sel}) || document.body;
                let tables = Array.from(document.querySelectorAll('table')).map(t => {{
                    let headers = Array.from(t.querySelectorAll('th')).map(th => th.textContent.trim());
                    let rowCount = t.querySelectorAll('tbody tr').length;
                    return {{ headers, rowCount }};
                }});
                let forms = Array.from(document.querySelectorAll('form')).map(f => {{
                    let inputs = Array.from(f.querySelectorAll('input, select, textarea')).map(i => i.name || i.id || i.type);
                    return {{ inputs }};
                }});
                let alerts = Array.from(document.querySelectorAll('.ant-alert, .ant-notification, [role="alert"]')).map(a => a.textContent.trim());
                return {{
                    url: window.location.href,
                    title: document.title,
                    textSnippet: (root.innerText || '').slice(0, 3000),
                    tables,
                    forms,
                    alerts
                }};
            }})()"#,
            sel = serde_json::to_string(selector).unwrap_or_default()
        );

        match session.evaluate(&js).await {
            Ok(res) => Ok(res),
            Err(e) => Ok(json!({ "success": false, "error": e })),
        }
    }
}

/// Tool: browser_get_console_errors
pub struct BrowserGetErrorsTool {
    parameters: Map<String, Value>,
}

impl BrowserGetErrorsTool {
    pub fn new() -> Self {
        Self {
            parameters: json!({
                "type": "object",
                "properties": {}
            })
            .as_object()
            .unwrap()
            .clone(),
        }
    }
}

impl Default for BrowserGetErrorsTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BrowserGetErrorsTool {
    fn name(&self) -> &str {
        "browser_get_console_errors"
    }

    fn description(&self) -> &str {
        "Checks for JavaScript runtime errors or captured window errors on the current page."
    }

    fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }

    async fn execute(&self, _args: Value) -> LlmResult<Value> {
        let mut mgr = GLOBAL_BROWSER.lock().await;
        let session = match mgr.ensure_browser(None).await {
            Ok(s) => s,
            Err(e) => return Ok(json!({ "success": false, "error": e })),
        };

        let js = r#"(() => {
            return {
                url: window.location.href,
                hasAntError: !!document.querySelector('.ant-result-error, .ant-alert-error'),
                errorText: document.querySelector('.ant-result-error, .ant-alert-error')?.innerText || null
            };
        })()"#;

        match session.evaluate(js).await {
            Ok(res) => Ok(res),
            Err(e) => Ok(json!({ "success": false, "error": e })),
        }
    }
}
