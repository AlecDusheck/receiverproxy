//! The API without a card: the router runs, nothing opens the link. Every
//! request here either never touches hardware or stops at the commit gate
//! before the link is opened.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use base64::prelude::{Engine, BASE64_STANDARD};
use daemon::{router, AppState};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fresh_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("daemon-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The daemon's token for every test; `req` presents it.
const TOKEN: &str = "s3cret";

fn state(name: &str) -> Arc<AppState> {
    AppState::new(fresh_dir(name), TOKEN.to_owned(), Some("nosuch0".to_owned())).unwrap()
}

fn req(method: Method, path: &str, body: Option<Value>) -> Request<Body> {
    let b = Request::builder()
        .method(method)
        .uri(path)
        .header("x-token", TOKEN);
    match body {
        Some(v) => b
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
}

async fn send(app: &Router, r: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let resp = app.clone().oneshot(r).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, headers, body)
}

async fn call(app: &Router, r: Request<Body>) -> (StatusCode, Value) {
    let (status, _, body) = send(app, r).await;
    let v: Value = serde_json::from_slice(&body)
        .unwrap_or_else(|e| panic!("not JSON ({e}): {}", String::from_utf8_lossy(&body)));
    (status, v)
}

async fn wait_job(app: &Router, id: &str) -> Value {
    for _ in 0..500 {
        let (status, job) = call(app, req(Method::GET, &format!("/api/v1/jobs/{id}"), None)).await;
        assert_eq!(status, StatusCode::OK);
        if job["state"] != "running" {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {id} did not finish");
}

#[tokio::test]
async fn health_has_the_documented_shape() {
    let app = router(state("health"));
    let (status, v) = call(&app, req(Method::GET, "/api/v1/health", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(v["iface"], "nosuch0");
    assert_eq!(v["cards"], json!([]));
}

#[tokio::test]
async fn config_gen_returns_what_the_cli_writes() {
    let spec_path = "config/panels/p25-128x64-sm16269s.toml";
    let spec_toml = std::fs::read_to_string(repo().join(spec_path)).unwrap();

    let app = router(state("gen"));
    let (status, v) = call(
        &app,
        req(
            Method::POST,
            "/api/v1/config/gen",
            Some(json!({ "spec_toml": spec_toml })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["name"], "p25-128x64-sm16269s");

    // `e120 config gen`, as the binary runs it from the repository root.
    let out = fresh_dir("gen-cli");
    let loader = |p: &str| ops::read_library(&repo().join(p).to_string_lossy());
    let mut lines = daemon::jobs::Lines::default();
    let g = ops::config::gen_config(
        ops::receivers::default_model(),
        &repo().join(spec_path).to_string_lossy(),
        &out.to_string_lossy(),
        "rcvbp",
        &loader,
        &mut lines,
    )
    .unwrap();
    let file = |name: &str| std::fs::read(out.join(format!("p25-128x64-sm16269s{name}"))).unwrap();
    let b64 = |k: &str| {
        BASE64_STANDARD
            .decode(v["files"][k].as_str().unwrap())
            .unwrap()
    };
    assert_eq!(b64("rcvbp"), file(".rcvbp"));
    assert_eq!(b64("basic_pack"), file("-basic-pack.bin"));
    assert_eq!(b64("block7"), file("-block7.bin"));
    assert_eq!(b64("block7").len(), 65536);
    let sources: Vec<String> = serde_json::from_value(v["sources"].clone()).unwrap();
    assert_eq!(sources, g.sources);
    let notes: Vec<String> = serde_json::from_value(v["notes"].clone()).unwrap();
    assert_eq!(notes, g.notes);
    // The report differs only in the spec line, which names the file.
    let cli_report = String::from_utf8(file("-sources.txt")).unwrap();
    let api_report = v["files"]["sources_txt"].as_str().unwrap();
    assert_eq!(
        cli_report.lines().skip(1).collect::<Vec<_>>(),
        api_report.lines().skip(1).collect::<Vec<_>>()
    );
    assert_eq!(lines.0.len(), 1, "one line naming the four paths");
}

#[tokio::test]
async fn a_gated_command_without_commit_returns_the_plan() {
    // A snapshot directory holding a config: `flash restore` prints its plan
    // and returns before opening the link.
    let snap = fresh_dir("gate-snap");
    let spec =
        ops::panelspec::PanelSpec::load(repo().join("config/panels/p25-128x64-sm16269s.toml"))
            .unwrap();
    let card = ops::receivers::default_model();
    let g = ops::config::generate(card, &spec, "spec.toml", "rcvbp", &daemon::state::load_library)
        .unwrap();
    let config = snap.join("config.rcvbp");
    std::fs::write(&config, &g.rcvbp).unwrap();

    let app = router(state("gate"));
    let (status, v) = call(
        &app,
        req(
            Method::POST,
            "/api/v1/flash/restore",
            Some(json!({ "dir": snap.to_string_lossy() })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let id = v["id"].as_str().unwrap().to_owned();

    let job = wait_job(&app, &id).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["kind"], "flash/restore");
    assert_eq!(job["result"]["committed"], false);
    let plan = format!(
        "dry run: {} -> parameter block (add --commit)",
        config.display()
    );
    assert_eq!(job["lines"], json!([{ "stream": "out", "text": plan }]));
    assert_eq!(job["result"]["lines"], job["lines"]);
    assert!(job["finished"].as_str().unwrap().ends_with('Z'));

    // The event stream replays the line and closes with the job.
    let (status, headers, body) = send(
        &app,
        req(Method::GET, &format!("/api/v1/jobs/{id}/events"), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
    let text = String::from_utf8(body).unwrap();
    assert!(
        text.contains("event: line\ndata: {\"stream\":\"out\""),
        "{text}"
    );
    assert!(text.contains("event: end\ndata: {\"id\":\"j1\""), "{text}");
}

#[tokio::test]
async fn a_running_job_makes_link_routes_409() {
    let st = state("busy");
    let app = router(st.clone());
    let id = st
        .start_job(daemon::jobs::JobKind::ShowHold, "fake", Vec::new(), |_, p| {
            while !p.cancelled() {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(None)
        })
        .unwrap();
    assert_eq!(id, "j1");

    let (status, v) = call(&app, req(Method::POST, "/api/v1/discover", None)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(v["error"], "job j1 (show/hold) is running");

    let (status, v) = call(
        &app,
        req(
            Method::POST,
            "/api/v1/flash/snapshot",
            Some(json!({ "dir": "x" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{v}");

    // Health never opens the link, so it is not gated.
    let (status, _) = call(&app, req(Method::GET, "/api/v1/health", None)).await;
    assert_eq!(status, StatusCode::OK);

    let (status, v) = call(&app, req(Method::DELETE, "/api/v1/jobs/j1", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["state"], "cancelled");

    // The link is free again: a job that fails before opening it runs.
    let (status, v) = call(
        &app,
        req(
            Method::POST,
            "/api/v1/flash/restore",
            Some(json!({ "dir": "/nonexistent/e120-snapshot" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let job = wait_job(&app, v["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "failed");
    assert_eq!(
        job["error"],
        "flash restore: /nonexistent/e120-snapshot holds neither primary-region.bin nor config.rcvbp"
    );

    let (_, jobs) = call(&app, req(Method::GET, "/api/v1/jobs", None)).await;
    let ids: Vec<&str> = jobs
        .as_array()
        .unwrap()
        .iter()
        .map(|j| j["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["j2", "j1"]);
}

#[tokio::test]
async fn cors_headers_are_present() {
    let app = router(state("cors"));
    let preflight = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/v1/health")
        .header(header::ORIGIN, "http://localhost:5173")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "PUT")
        .header(
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "content-type, x-token",
        )
        .body(Body::empty())
        .unwrap();
    let (status, headers, _) = send(&app, preflight).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_METHODS], "*");
    let allowed = headers[header::ACCESS_CONTROL_ALLOW_HEADERS]
        .to_str()
        .unwrap();
    assert!(
        allowed.contains("content-type") && allowed.contains("x-token"),
        "{allowed}"
    );

    let simple = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/health")
        .header(header::ORIGIN, "http://localhost:5173")
        .body(Body::empty())
        .unwrap();
    let (status, headers, _) = send(&app, simple).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
}

#[tokio::test]
async fn the_token_gates_every_route_but_health() {
    let app = router(state("token"));
    let bare = |path: &str| Request::builder().uri(path).body(Body::empty()).unwrap();
    let with = |path: &str, token: &str| {
        Request::builder()
            .uri(path)
            .header("x-token", token)
            .body(Body::empty())
            .unwrap()
    };

    let (status, v) = call(&app, bare("/api/v1/settings")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(v["error"], "token required");
    let (status, v) = call(&app, with("/api/v1/settings", "nope")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(v["error"], "bad token");
    let (status, v) = call(&app, with("/api/v1/settings", TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["iface"], "nosuch0");

    // Health answers without a token, with the version alone.
    let (status, v) = call(&app, bare("/api/v1/health")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v, json!({ "version": env!("CARGO_PKG_VERSION") }));
    let (status, v) = call(&app, with("/api/v1/health", "nope")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v, json!({ "version": env!("CARGO_PKG_VERSION") }));
    let (status, v) = call(&app, with("/api/v1/health", TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["iface"], "nosuch0");
    assert_eq!(v["cards"], json!([]));

    // `EventSource` cannot set headers, so the query form counts too.
    let (status, v) = call(&app, bare("/api/v1/jobs?token=s3cret")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v, json!([]));
    let (status, _) = call(&app, bare("/api/v1/jobs?token=nope")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn errors_are_json_with_the_documented_codes() {
    let app = router(state("errors"));
    let (status, v) = call(&app, req(Method::GET, "/api/v1/nope", None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(v["error"], "not found");

    let (status, v) = call(&app, req(Method::GET, "/api/v1/jobs/j77", None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(v["error"], "no job j77");

    let bad = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/brightness")
        .header("x-token", TOKEN)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{\"value\": 999}"))
        .unwrap();
    let (status, v) = call(&app, bad).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(v["error"].as_str().unwrap().starts_with("body: "), "{v}");

    // Without the web app built, the root explains how to build it.
    let (status, _, body) = send(&app, req(Method::GET, "/", None)).await;
    assert_eq!(status, StatusCode::OK);
    let text = String::from_utf8(body).unwrap();
    assert!(
        text.starts_with("build the web app") || text.contains("<html"),
        "{text}"
    );
}

#[tokio::test]
async fn the_wall_is_validated_and_stored() {
    let st = state("wall");
    let app = router(st.clone());
    let (status, v) = call(&app, req(Method::GET, "/api/v1/wall", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["width"], 128);

    let two = wall::Canvas::cards(128, 64, 2, 1);
    let (status, v) = call(
        &app,
        req(
            Method::PUT,
            "/api/v1/wall",
            Some(serde_json::to_value(&two).unwrap()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["width"], 256);
    assert!(st.data_dir.join("wall.json").is_file());

    let mut broken = two.clone();
    broken.panels[0].receiver = 9;
    let (status, v) = call(
        &app,
        req(
            Method::PUT,
            "/api/v1/wall",
            Some(serde_json::to_value(&broken).unwrap()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        v["error"]
            .as_str()
            .unwrap()
            .starts_with("canvas is not valid"),
        "{v}"
    );

    let (status, v) = call(
        &app,
        req(
            Method::PUT,
            "/api/v1/settings",
            Some(json!({ "iface": "eth0", "brightness": 40 })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (_, h) = call(&app, req(Method::GET, "/api/v1/health", None)).await;
    assert_eq!(h["iface"], "eth0");
}
