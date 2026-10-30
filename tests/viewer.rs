mod common;
use common::Fixture;
use git_chronicle::analysis::{Granularity, Options, analyze};
use git_chronicle::config::Config;
use git_chronicle::git::Repo;
use git_chronicle::viewer::render_html;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::Duration;

fn repo() -> Fixture {
    let mut f = Fixture::new();
    f.write("src/a.ts", "import './b'\n");
    f.write("src/b.ts", "export {}\n");
    f.commit("Alice", "evil </script><img src=x onerror=alert(1)> subject");
    f
}

fn html(f: &Fixture, theme: &str, overlay: &str) -> anyhow::Result<String> {
    let repo = Repo::open(f.path()).unwrap();
    let o = Options {
        granularity: Some(Granularity::Commit),
        ..Options::default()
    };
    render_html(&analyze(&repo, &Config::default(), &o).unwrap(), theme, overlay)
}

#[test]
fn html_embeds_timeline_and_cannot_be_broken_out_of() {
    let h = html(&repo(), "github-dark", "architecture").unwrap();
    assert!(h.contains("<canvas"));
    assert!(h.contains("src/a.ts"));
    assert_eq!(
        h.matches("</script>").count(),
        2,
        "only our own script tags may close"
    );
    assert!(
        !h.contains("<img src=x"),
        "commit subject must be escaped inside the page data"
    );
    assert!(!h.contains("__CHRONICLE_DATA__"));
}

#[test]
fn unknown_theme_or_overlay_is_rejected() {
    let f = repo();
    assert!(
        html(&f, "neon-pink", "architecture")
            .unwrap_err()
            .to_string()
            .contains("theme")
    );
    assert!(
        html(&f, "github-dark", "rainbow")
            .unwrap_err()
            .to_string()
            .contains("overlay")
    );
}

#[test]
fn cli_exports_standalone_html() {
    let f = repo();
    let out = f.path().join("t.html");
    let o = Command::new(env!("CARGO_BIN_EXE_git-chronicle"))
        .args([
            "--export",
            out.to_str().unwrap(),
            "--theme",
            "cyberpunk",
            "--overlay",
            "cycles",
        ])
        .current_dir(f.path())
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let h = std::fs::read_to_string(out).unwrap();
    assert!(h.contains("<canvas") && h.contains("cyberpunk") && h.contains("\"cycles\""));
}

#[test]
fn cli_serves_the_viewer_on_localhost() {
    let f = repo();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_git-chronicle"))
        .args(["--no-open", "--port", &port.to_string()])
        .current_dir(f.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut body = String::new();
    for _ in 0..100 {
        if let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) {
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            s.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
            s.read_to_string(&mut body).unwrap();
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(body.starts_with("HTTP/1.1 200"), "{body:.200}");
    assert!(body.contains("<canvas"));
}

#[test]
fn viewer_script_runs_against_a_stub_dom() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("skipping: node not installed");
        return;
    }
    let mut f = Fixture::new();
    f.write("src/a.ts", "import './b'\n");
    f.write("src/b.ts", "export {}\n");
    f.commit("Alice", "c1");
    f.write("src/c.ts", "import './a'\n");
    f.commit("Alice", "c2");
    f.write("src/b.ts", "import './c'\n");
    f.commit("Bob", "c3 closes the loop");
    let page = f.path().join("page.html");
    let o = Command::new(env!("CARGO_BIN_EXE_git-chronicle"))
        .args(["--export", page.to_str().unwrap()])
        .current_dir(f.path())
        .output()
        .unwrap();
    assert!(o.status.success());
    let o = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/viewer_smoke.js"))
        .arg(&page)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}
