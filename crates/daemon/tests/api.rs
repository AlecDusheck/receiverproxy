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

    // `rxp config gen`, as the binary runs it from the repository root.
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
            Some(json!({ "dir": "/nonexistent/rxp-snapshot" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let job = wait_job(&app, v["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "failed");
    assert_eq!(
        job["error"],
        "flash restore: /nonexistent/rxp-snapshot holds neither primary-region.bin nor config.rcvbp"
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

/// A 12-byte `RXP\0` stream header, as `sources::raw::Header` writes it.
fn frame_header(w: u16, h: u16, fps: u16) -> Vec<u8> {
    sources::raw::Header {
        width: w,
        height: h,
        fps,
    }
    .to_bytes()
    .to_vec()
}

#[tokio::test]
async fn the_state_follows_the_shows_and_the_jobs() {
    let st = state("live");
    let app = router(st.clone());

    // Nothing has been shown yet; the brightness is the settings'.
    let (status, v) = call(&app, req(Method::GET, "/api/v1/state", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v, json!({ "show": null, "brightness": 255, "cards": [], "job": null }));

    // A still without a card fails at the link, but the state records what
    // went up: the daemon's own view of the panel, not the card's answer.
    st.showing(
        daemon::api::ShowKind::Pattern,
        "rgb".to_owned(),
        None,
        None,
    );
    let (_, v) = call(&app, req(Method::GET, "/api/v1/state", None)).await;
    assert_eq!(v["show"]["kind"], "pattern");
    assert_eq!(v["show"]["source"], "rgb");
    assert_eq!(v["show"]["fps"], Value::Null);
    assert_eq!(v["show"]["layout"], "128x64, 1 card");
    assert_eq!(v["show"]["job"], Value::Null);
    assert!(v["show"]["started"].as_str().unwrap().ends_with('Z'));

    // A job's start and end both show up, and the show a job held leaves with it.
    let id = st
        .start_job(daemon::jobs::JobKind::ShowHold, "fake", Vec::new(), |_, p| {
            while !p.cancelled() {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(None)
        })
        .unwrap();
    st.showing(
        daemon::api::ShowKind::Still,
        "held".to_owned(),
        None,
        Some(id.clone()),
    );
    let (_, v) = call(&app, req(Method::GET, "/api/v1/state", None)).await;
    assert_eq!(v["job"], json!({ "id": "j1", "kind": "show/hold", "state": "running", "started": v["job"]["started"] }));
    assert_eq!(v["show"]["job"], "j1");

    let (status, _) = call(&app, req(Method::DELETE, "/api/v1/jobs/j1", None)).await;
    assert_eq!(status, StatusCode::OK);
    // The watcher publishes from its own task; the job is already finished.
    for _ in 0..200 {
        let (_, v) = call(&app, req(Method::GET, "/api/v1/state", None)).await;
        if v["job"]["state"] == "cancelled" {
            assert_eq!(v["show"], Value::Null, "the show left with its job");
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the live state never saw the job end");
}

#[tokio::test]
async fn the_state_stream_sends_the_state_object_at_once() {
    let st = state("live-sse");
    let app = router(st.clone());
    // The stream never ends, so only its first event is read.
    let resp = app
        .clone()
        .oneshot(req(Method::GET, "/api/v1/state/events", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
    let mut body = resp.into_body();
    let mut text = String::new();
    while !text.contains("\n\n") {
        let frame = body.frame().await.expect("the stream closed").unwrap();
        if let Some(d) = frame.data_ref() {
            text.push_str(&String::from_utf8_lossy(d));
        }
    }
    let data = text
        .strip_prefix("event: state\ndata: ")
        .and_then(|t| t.split('\n').next())
        .unwrap_or_else(|| panic!("{text}"));
    let v: Value = serde_json::from_str(data).unwrap();
    assert_eq!(
        v,
        serde_json::to_value(st.live.snapshot()).unwrap(),
        "the stream's first event is GET /state"
    );
}

#[tokio::test]
async fn a_frame_is_refused_unless_its_header_matches_its_body() {
    let app = router(state("frame"));
    let post = |body: Vec<u8>| {
        Request::builder()
            .method(Method::POST)
            .uri("/api/v1/show/frame")
            .header("x-token", TOKEN)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(Body::from(body))
            .unwrap()
    };

    let (status, v) = call(&app, post(b"RXP".to_vec())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(v["error"], "frame: 3 bytes, shorter than the 12-byte header");

    let mut wrong_magic = frame_header(4, 4, 30);
    wrong_magic[0] = b'X';
    wrong_magic.extend(std::iter::repeat_n(0u8, 4 * 4 * 3));
    let (status, v) = call(&app, post(wrong_magic)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(v["error"], "frame: not a receiverproxy stream header");

    let mut zero = frame_header(0, 4, 30);
    zero.extend([0u8; 12]);
    let (status, v) = call(&app, post(zero)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(v["error"], "frame: zero-sized stream");

    let mut short = frame_header(4, 4, 30);
    short.extend(std::iter::repeat_n(0u8, 4 * 4 * 3 - 1));
    let (status, v) = call(&app, post(short)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        v["error"],
        "frame: 59 bytes, header says 4x4 rgb24 is 60"
    );
}

#[tokio::test]
async fn an_uploaded_firmware_is_checked_against_the_manifest() {
    let st = state("upload");
    let app = router(st.clone());
    let listed = &receivers::firmware::manifest().image[0];
    let size = receivers::firmware::manifest().size as usize;

    let post = |name: &str, body: Vec<u8>| {
        Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1/firmware/upload?name={name}"))
            .header("x-token", TOKEN)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(Body::from(body))
            .unwrap()
    };

    // A manifest name whose bytes are not the manifest's is refused.
    let (status, v) = call(&app, post(&listed.name, vec![b'x'; size])).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err = v["error"].as_str().unwrap();
    assert!(err.starts_with("firmware upload: "), "{err}");
    assert!(err.contains("manifest says"), "{err}");
    assert!(
        !st.data_dir.join("firmware").join(&listed.name).exists(),
        "a refused image is not written"
    );

    // Anything else is kept as it is, with its own hash reported.
    let (status, v) = call(&app, post("mine.hex", b"hello".to_vec())).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["name"], "mine.hex");
    assert_eq!(v["size"], 5);
    assert_eq!(v["verified"], false);
    assert_eq!(v["manifest_sha256"], Value::Null);
    assert_eq!(v["sha256"], receivers::firmware::sha256_hex(b"hello"));
    let written = st.data_dir.join("firmware").join("mine.hex");
    assert_eq!(std::fs::read(&written).unwrap(), b"hello");
    assert_eq!(v["path"], written.to_string_lossy().as_ref());
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
