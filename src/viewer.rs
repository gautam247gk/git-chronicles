//! Self-contained HTML viewer: one embedded canvas page with the timeline JSON inlined.
use crate::analysis::Analysis;
use anyhow::{Result, bail};
use std::io::{Read, Write};
use std::net::TcpListener;

const TEMPLATE: &str = include_str!("../assets/viewer.html");
pub const THEMES: [&str; 5] = [
    "github-dark",
    "cyberpunk",
    "catppuccin",
    "dracula",
    "minimal-light",
];
pub const OVERLAYS: [&str; 5] = ["architecture", "hotspots", "bus-factor", "cycles", "orphans"];

pub fn render_html(a: &Analysis, theme: &str, overlay: &str) -> Result<String> {
    if !THEMES.contains(&theme) {
        bail!("unknown theme {theme:?} (expected one of {})", THEMES.join(", "));
    }
    if !OVERLAYS.contains(&overlay) {
        bail!(
            "unknown overlay {overlay:?} (expected one of {})",
            OVERLAYS.join(", ")
        );
    }
    let data = serde_json::json!({"timeline": a, "theme": theme, "overlay": overlay}).to_string();
    // `<` as < keeps commit messages from closing the script tag or opening comments
    Ok(TEMPLATE.replacen("__CHRONICLE_DATA__", &data.replace('<', "\\u003c"), 1))
}

/// Serve `html` on 127.0.0.1:`port` until the process is killed. Single-threaded and sequential.
pub fn serve(html: &str, port: u16, announce: impl FnOnce(&str)) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| anyhow::anyhow!("cannot bind 127.0.0.1:{port}: {e} (try --port)"))?;
    announce(&format!("http://127.0.0.1:{port}"));
    for stream in listener.incoming() {
        let Ok(mut s) = stream else { continue };
        let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(2)));
        let mut buf = [0u8; 2048];
        let n = s.read(&mut buf).unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]);
        let ok =
            req.starts_with("GET / ") || req.starts_with("GET /index.html ") || req.starts_with("GET /?");
        let (status, body) = if ok {
            ("200 OK", html)
        } else {
            ("404 Not Found", "not found")
        };
        let _ = write!(
            s,
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    }
    Ok(())
}

pub fn open_browser(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    if std::process::Command::new(opener)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_err()
    {
        eprintln!("could not launch a browser; open {url} manually");
    }
}
