//! One handler per route in `docs/ui.md` section 2.

use crate::assets;
use crate::error::{ApiError, ApiResult};
use crate::jobs::{Job, Line, Outcome};
use crate::state::{load_library, AppState, Settings};
use crate::{lock, Shared};
use anyhow::Context;
use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Multipart, Path, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderName};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::prelude::{Engine, BASE64_STANDARD};
use e120_canvas::Canvas;
use e120_commands::{capture, config, display, flash, params, provision, restore, screen, upgrade};
use e120_proto::DiscoveryInfo;
use e120_rcvbp::spec::PanelSpec;
use e120_video::Fit;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

/// The whole application: the API under `/api/v1`, the web app elsewhere.
pub fn router(state: Shared) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/discover", post(discover))
        .route("/settings", get(get_settings).put(put_settings))
        .route("/brightness", post(brightness))
        .route("/show/image", post(show_image))
        .route("/show/video", post(show_video))
        .route("/show/pattern", post(show_pattern))
        .route("/show/fill", post(show_fill))
        .route("/show/blank", post(show_blank))
        .route("/config/gen", post(config_gen))
        .route("/config/read", post(config_read))
        .route("/config/write", post(config_write))
        .route("/config/send", post(config_send))
        .route("/provision", post(provision_route))
        .route("/flash/snapshot", post(flash_snapshot))
        .route("/flash/restore", post(flash_restore))
        .route("/firmware/install", post(firmware_install))
        .route(
            "/card/screen-size",
            get(get_screen_size).put(put_screen_size),
        )
        .route("/card/reload", post(card_reload))
        .route("/card/test-mode", post(card_test_mode))
        .route("/card/set-layout", post(card_set_layout))
        .route("/wall", get(get_wall).put(put_wall))
        .route("/jobs", get(list_jobs))
        .route("/jobs/{id}", get(get_job).delete(delete_job))
        .route("/jobs/{id}/events", get(job_events))
        .layer(middleware::from_fn_with_state(state.clone(), token_check));
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers([header::CONTENT_TYPE, HeaderName::from_static("x-token")]);
    Router::new()
        .nest("/api/v1", api)
        .fallback(assets::fallback)
        .layer(cors)
        .with_state(state)
}

/// `X-Token` must match `--token` when one is set.
async fn token_check(State(state): State<Shared>, req: Request, next: Next) -> Response {
    if let Some(want) = &state.token {
        match req.headers().get("x-token").and_then(|v| v.to_str().ok()) {
            None => return ApiError::unauthorized("token required").into_response(),
            Some(got) if got != want => return ApiError::unauthorized("bad token").into_response(),
            Some(_) => {}
        }
    }
    next.run(req).await
}

// --- extractors -------------------------------------------------------------

/// A JSON body, or `{}` when the body is empty; a parse failure is 400.
struct Body<T>(T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for Body<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, s: &S) -> ApiResult<Self> {
        let bytes = Bytes::from_request(req, s)
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?;
        let value = if bytes.is_empty() {
            serde_json::from_str("{}")
        } else {
            serde_json::from_slice(&bytes)
        };
        value
            .map(Body)
            .map_err(|e| ApiError::bad_request(format!("body: {e}")))
    }
}

/// A query string; a parse failure is 400.
struct Qs<T>(T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequestParts<S> for Qs<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, s: &S) -> ApiResult<Self> {
        Query::<T>::from_request_parts(parts, s)
            .await
            .map(|Query(v)| Self(v))
            .map_err(|e| ApiError::bad_request(e.body_text()))
    }
}

// --- shapes -----------------------------------------------------------------

/// `e120_proto::DiscoveryInfo` without `raw`; the field names are the API.
#[derive(Serialize)]
#[allow(clippy::struct_field_names)]
struct Card {
    controller: u8,
    card_id: u8,
    ver_major: u8,
    ver_minor: u8,
    cols: u16,
    rows: u16,
}

impl From<&DiscoveryInfo> for Card {
    fn from(i: &DiscoveryInfo) -> Self {
        Self {
            controller: i.controller,
            card_id: i.card_id,
            ver_major: i.ver_major,
            ver_minor: i.ver_minor,
            cols: i.cols,
            rows: i.rows,
        }
    }
}

#[derive(Serialize)]
struct Health {
    version: &'static str,
    iface: String,
    cards: Vec<Card>,
}

#[derive(Serialize)]
struct Started {
    id: String,
}

fn outcome(lines: Vec<Line>, files: Vec<String>, committed: Option<bool>) -> Json<Outcome> {
    Json(Outcome {
        lines,
        files,
        committed,
    })
}

fn wait_default() -> u64 {
    3
}

fn wait_2() -> u64 {
    2
}

fn fps_default() -> u32 {
    30
}

fn contain() -> Fit {
    Fit::Contain
}

fn parse_fit(s: &str) -> ApiResult<Fit> {
    s.parse()
        .map_err(|e| ApiError::bad_request(format!("fit: {e}")))
}

/// `Fit` from its CLI spelling.
fn de_fit<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Fit, D::Error> {
    let s: String = Deserialize::deserialize(d)?;
    s.parse().map_err(serde::de::Error::custom)
}

fn de_fit_opt<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Fit, D::Error> {
    de_fit(d)
}

/// A file name for a snapshot or backup, from the clock.
fn stamp() -> u64 {
    AppState::unix_seconds()
}

fn data_path(state: &AppState, sub: &str, name: &str) -> ApiResult<String> {
    let dir = state.data_dir.join(sub);
    std::fs::create_dir_all(&dir).map_err(|e| {
        ApiError::command(
            "data dir",
            &anyhow::anyhow!("create {}: {e}", dir.display()),
        )
    })?;
    Ok(dir.join(name).to_string_lossy().into_owned())
}

// --- health, discovery, settings -------------------------------------------

async fn health(State(state): State<Shared>) -> Json<Health> {
    Json(Health {
        version: env!("CARGO_PKG_VERSION"),
        iface: state.settings().iface,
        cards: lock(&state.cards).iter().map(Card::from).collect(),
    })
}

#[derive(Deserialize)]
struct DiscoverReq {
    #[serde(default = "wait_default")]
    wait: u64,
}

async fn discover(
    State(state): State<Shared>,
    Body(req): Body<DiscoverReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let wait = req.wait;
    let (cards, _) = state
        .command("discover", move |ctx, p| capture::discover(ctx, wait, p))
        .await?;
    let out: Vec<Card> = cards.iter().map(Card::from).collect();
    *lock(&state.cards) = cards;
    Ok(Json(serde_json::json!({ "cards": out })))
}

async fn get_settings(State(state): State<Shared>) -> Json<Settings> {
    Json(state.settings())
}

async fn put_settings(
    State(state): State<Shared>,
    Body(s): Body<Settings>,
) -> ApiResult<Json<Settings>> {
    if s.iface.trim().is_empty() {
        return Err(ApiError::bad_request("iface: empty"));
    }
    state
        .set_settings(s.clone())
        .map_err(|e| ApiError::command("settings", &e))?;
    Ok(Json(s))
}

#[derive(Serialize, Deserialize)]
struct BrightnessReq {
    value: u8,
}

async fn brightness(
    State(state): State<Shared>,
    Body(req): Body<BrightnessReq>,
) -> ApiResult<Json<BrightnessReq>> {
    let value = req.value;
    state
        .command("brightness", move |ctx, _| display::brightness(ctx, value))
        .await?;
    let mut s = state.settings();
    s.brightness = value;
    state
        .set_settings(s)
        .map_err(|e| ApiError::command("brightness", &e))?;
    Ok(Json(BrightnessReq { value }))
}

// --- show -------------------------------------------------------------------

/// Three refreshes as a command, or a `show/hold` job; either replaces a
/// running show job.
async fn show_still(
    state: &Shared,
    subject: &'static str,
    canvas: Canvas,
    frame: e120_canvas::Frame,
    hold: bool,
) -> ApiResult<Response> {
    state.cancel_show().await;
    if hold {
        let id = state.start_job("show/hold", subject, Vec::new(), move |ctx, p| {
            display::show_frame(ctx, canvas, &frame, true, p).map(|()| None)
        })?;
        return Ok(Json(Started { id }).into_response());
    }
    let ((), lines) = state
        .command(subject, move |ctx, p| {
            display::show_frame(ctx, canvas, &frame, false, p)
        })
        .await?;
    Ok(outcome(lines, Vec::new(), None).into_response())
}

#[derive(Deserialize)]
struct ShowImageReq {
    path: String,
    #[serde(default, deserialize_with = "de_fit_opt")]
    fit: Fit,
    #[serde(default)]
    hold: bool,
}

/// A multipart `show/image`: the `file` part's bytes and name, `fit`, `hold`.
async fn image_upload(mut mp: Multipart) -> ApiResult<(Bytes, String, Fit, bool)> {
    let bad = |e: axum::extract::multipart::MultipartError| ApiError::bad_request(e.body_text());
    let (mut file, mut name, mut fit, mut hold) = (None, String::new(), Fit::Stretch, false);
    while let Some(field) = mp.next_field().await.map_err(bad)? {
        match field.name().unwrap_or("") {
            "file" => {
                name = field.file_name().unwrap_or("upload").to_owned();
                file = Some(field.bytes().await.map_err(bad)?);
            }
            "fit" => fit = parse_fit(&field.text().await.map_err(bad)?)?,
            "hold" => hold = field.text().await.map_err(bad)? == "true",
            _ => {}
        }
    }
    let bytes = file.ok_or_else(|| ApiError::bad_request("multipart: no file part"))?;
    Ok((bytes, name, fit, hold))
}

async fn show_image(State(state): State<Shared>, req: Request) -> ApiResult<Response> {
    let multipart = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("multipart/form-data"));
    let (img, fit, hold) = if multipart {
        let mp = Multipart::from_request(req, &state)
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?;
        let (bytes, name, fit, hold) = image_upload(mp).await?;
        let img = image::load_from_memory(&bytes)
            .with_context(|| format!("decode image {name}"))
            .map_err(|e| ApiError::command("show image", &e))?;
        (img, fit, hold)
    } else {
        let Body(r) = Body::<ShowImageReq>::from_request(req, &state).await?;
        let img = image::open(&r.path)
            .with_context(|| format!("open image {}", r.path))
            .map_err(|e| ApiError::command("show image", &e))?;
        (img, r.fit, r.hold)
    };
    let canvas = state.wall();
    let frame = display::image_frame(&img, &canvas, fit)
        .map_err(|e| ApiError::command("show image", &e))?;
    show_still(&state, "show image", canvas, frame, hold).await
}

#[derive(Deserialize)]
struct ShowVideoReq {
    path: String,
    #[serde(default, rename = "loop")]
    looping: bool,
    #[serde(default = "fps_default")]
    fps: u32,
    #[serde(default = "contain", deserialize_with = "de_fit")]
    fit: Fit,
    layout: Option<Canvas>,
}

async fn show_video(
    State(state): State<Shared>,
    Body(req): Body<ShowVideoReq>,
) -> ApiResult<Json<Started>> {
    let canvas = match req.layout {
        Some(c) => {
            c.validate()
                .map_err(|e| ApiError::bad_request(e.to_string()))?;
            c
        }
        None => state.wall(),
    };
    state.cancel_show().await;
    let id = state.start_job("show/video", "show video", Vec::new(), move |ctx, p| {
        display::play_on(ctx, canvas, &req.path, req.fps, req.fit, req.looping, p).map(|()| None)
    })?;
    Ok(Json(Started { id }))
}

#[derive(Deserialize)]
struct ShowPatternReq {
    name: String,
    #[serde(default)]
    hold: bool,
}

async fn show_pattern(
    State(state): State<Shared>,
    Body(req): Body<ShowPatternReq>,
) -> ApiResult<Response> {
    let pattern: e120_video::Pattern = req
        .name
        .parse()
        .map_err(|e| ApiError::bad_request(format!("name: {e}")))?;
    let canvas = state.wall();
    let frame = e120_video::pattern(pattern, canvas.width, canvas.height);
    show_still(&state, "show pattern", canvas, frame, req.hold).await
}

#[derive(Deserialize)]
struct ShowFillReq {
    rgb: String,
    #[serde(default)]
    hold: bool,
}

async fn show_fill(
    State(state): State<Shared>,
    Body(req): Body<ShowFillReq>,
) -> ApiResult<Response> {
    let rgb = e120_commands::util::parse_color(std::slice::from_ref(&req.rgb))
        .map_err(|e| ApiError::bad_request(format!("rgb: {e:#}")))?;
    let canvas = state.wall();
    let frame = solid(&canvas, rgb)?;
    show_still(&state, "show fill", canvas, frame, req.hold).await
}

fn solid(canvas: &Canvas, rgb: [u8; 3]) -> ApiResult<e120_canvas::Frame> {
    e120_canvas::Frame::from_rgb(
        canvas.width,
        canvas.height,
        rgb.repeat((canvas.width * canvas.height) as usize),
    )
    .map_err(|e| ApiError::command("show fill", &anyhow::anyhow!("{e:?}")))
}

async fn show_blank(State(state): State<Shared>) -> ApiResult<Response> {
    let canvas = state.wall();
    let frame = solid(&canvas, [0, 0, 0])?;
    show_still(&state, "show blank", canvas, frame, false).await
}

// --- config -----------------------------------------------------------------

#[derive(Deserialize)]
struct SpecReq {
    spec_toml: String,
}

fn parse_spec(subject: &str, toml: &str) -> ApiResult<PanelSpec> {
    PanelSpec::parse(toml)
        .context("parse spec")
        .map_err(|e| ApiError::command(subject, &e))
}

async fn config_gen(Body(req): Body<SpecReq>) -> ApiResult<Json<serde_json::Value>> {
    let spec = parse_spec("config gen", &req.spec_toml)?;
    let label = format!("{}.toml", spec.name);
    let g = config::generate(&spec, &label, &load_library)
        .map_err(|e| ApiError::command("config gen", &e))?;
    Ok(Json(serde_json::json!({
        "name": g.name,
        "files": {
            "rcvbp": BASE64_STANDARD.encode(&g.rcvbp),
            "basic_pack": BASE64_STANDARD.encode(&g.basic_pack),
            "block7": g.block7.as_deref().map(|b| BASE64_STANDARD.encode(b)),
            "sources_txt": g.report,
        },
        "sources": g.sources,
        "notes": g.notes,
    })))
}

#[derive(Deserialize)]
struct ConfigReadReq {
    #[serde(default)]
    index: u16,
    #[serde(default = "basic_param_page")]
    page: u16,
    #[serde(default = "chunks_default")]
    max_chunks: u16,
    #[serde(default = "wait_2")]
    wait: u64,
}

fn basic_param_page() -> u16 {
    e120_proto::FLASH_PAGE_BASIC_PARAM
}

fn chunks_default() -> u16 {
    64
}

async fn config_read(
    State(state): State<Shared>,
    Body(r): Body<ConfigReadReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let (bytes, lines) = state
        .command("config read", move |ctx, _| {
            flash::read_config(ctx, r.index, r.page, r.max_chunks, r.wait)
        })
        .await?;
    Ok(Json(serde_json::json!({
        "rcvbp": BASE64_STANDARD.encode(&bytes),
        "lines": lines,
    })))
}

#[derive(Deserialize)]
struct ConfigWriteReq {
    rcvbp: String,
    #[serde(default)]
    commit: bool,
    #[serde(default)]
    index: u16,
    #[serde(default = "wait_2")]
    wait: u64,
}

async fn config_write(
    State(state): State<Shared>,
    Body(r): Body<ConfigWriteReq>,
) -> ApiResult<Json<Outcome>> {
    let bytes = BASE64_STANDARD
        .decode(&r.rcvbp)
        .map_err(|e| ApiError::bad_request(format!("rcvbp: {e}")))?;
    let ts = stamp();
    let config = data_path(&state, "backups", &format!("config-{ts}.rcvbp"))?;
    std::fs::write(&config, &bytes)
        .map_err(|e| ApiError::command("config write", &anyhow::anyhow!("write {config}: {e}")))?;
    let backup = data_path(&state, "backups", &format!("block07-{ts}.bin"))?;
    let files = vec![config.clone(), backup.clone()];
    let ((), lines) = state
        .command("config write", move |ctx, p| {
            flash::write_config(ctx, &config, r.commit, &backup, None, r.index, r.wait, p)
        })
        .await?;
    Ok(outcome(lines, files, Some(r.commit)))
}

#[derive(Deserialize)]
struct ConfigSendReq {
    spec_toml: String,
    #[serde(default)]
    chip_only: bool,
    #[serde(default = "gap_default")]
    gap_ms: u64,
}

fn gap_default() -> u64 {
    8
}

async fn config_send(
    State(state): State<Shared>,
    Body(r): Body<ConfigSendReq>,
) -> ApiResult<Json<Outcome>> {
    let spec = parse_spec("config send", &r.spec_toml)?;
    let g = spec
        .chip_library(&load_library)
        .and_then(|chip| spec.generate_with(&chip))
        .map_err(|e| ApiError::command("config send", &e))?;
    let ((), lines) = state
        .command("config send", move |ctx, _| {
            params::send_generated(ctx, &spec, &g, r.chip_only, r.gap_ms)
        })
        .await?;
    Ok(outcome(lines, Vec::new(), None))
}

// --- provision, flash, firmware ---------------------------------------------

#[derive(Deserialize)]
struct ProvisionReq {
    spec_toml: String,
    firmware_path: Option<String>,
    position: (u16, u16),
    snapshot_dir: Option<String>,
    #[serde(default)]
    commit: bool,
    #[serde(default = "wait_default")]
    wait: u64,
}

async fn provision_route(
    State(state): State<Shared>,
    Body(r): Body<ProvisionReq>,
) -> ApiResult<Json<Started>> {
    let dir = match r.snapshot_dir {
        Some(d) => d,
        None => data_path(&state, "snapshots", &stamp().to_string())?,
    };
    std::fs::create_dir_all(&dir)
        .map_err(|e| ApiError::command("provision", &anyhow::anyhow!("create {dir}: {e}")))?;
    let spec_path = format!("{dir}/spec.toml");
    std::fs::write(&spec_path, &r.spec_toml)
        .map_err(|e| ApiError::command("provision", &anyhow::anyhow!("write {spec_path}: {e}")))?;
    let files = vec![spec_path.clone()];
    let id = state.start_job("provision", "provision", files, move |ctx, p| {
        provision::provision(
            ctx,
            &provision::Args {
                spec_path: &spec_path,
                firmware: r.firmware_path.as_deref(),
                position: r.position,
                snapshot_dir: Some(&dir),
                commit: r.commit,
                wait: r.wait,
            },
            &load_library,
            p,
        )
        .map(|()| Some(r.commit))
    })?;
    Ok(Json(Started { id }))
}

#[derive(Deserialize)]
struct SnapshotReq {
    dir: Option<String>,
    #[serde(default)]
    index: u16,
    #[serde(default = "wait_default")]
    wait: u64,
}

async fn flash_snapshot(
    State(state): State<Shared>,
    Body(r): Body<SnapshotReq>,
) -> ApiResult<Json<Started>> {
    let dir = match r.dir {
        Some(d) => d,
        None => data_path(&state, "snapshots", &stamp().to_string())?,
    };
    let files = vec![
        format!("{dir}/primary-region.bin"),
        format!("{dir}/golden-bank.bin"),
    ];
    let id = state.start_job("flash/snapshot", "flash snapshot", files, move |ctx, p| {
        restore::snapshot(ctx, &dir, r.index, r.wait, p).map(|()| None)
    })?;
    Ok(Json(Started { id }))
}

#[derive(Deserialize)]
struct RestoreReq {
    dir: String,
    #[serde(default)]
    commit: bool,
    #[serde(default)]
    index: u16,
    #[serde(default = "wait_default")]
    wait: u64,
}

async fn flash_restore(
    State(state): State<Shared>,
    Body(r): Body<RestoreReq>,
) -> ApiResult<Json<Started>> {
    let id = state.start_job(
        "flash/restore",
        "flash restore",
        Vec::new(),
        move |ctx, p| {
            restore::all(ctx, &r.dir, r.commit, r.index, r.wait, p).map(|()| Some(r.commit))
        },
    )?;
    Ok(Json(Started { id }))
}

#[derive(Deserialize)]
struct FirmwareReq {
    path: String,
    #[serde(default)]
    commit: bool,
    #[serde(default)]
    golden: bool,
    #[serde(default = "timeout_default")]
    timeout: u64,
    #[serde(default = "chunk_delay_default")]
    chunk_delay_us: u64,
    #[serde(default = "wait_4")]
    wait: u64,
}

fn timeout_default() -> u64 {
    120
}

fn chunk_delay_default() -> u64 {
    3000
}

fn wait_4() -> u64 {
    4
}

async fn firmware_install(
    State(state): State<Shared>,
    Body(r): Body<FirmwareReq>,
) -> ApiResult<Json<Started>> {
    let partition = if r.golden {
        e120_proto::upgrade::Partition::Golden
    } else {
        e120_proto::upgrade::Partition::Primary
    };
    let id = state.start_job(
        "firmware/install",
        "firmware install",
        Vec::new(),
        move |ctx, p| {
            upgrade::install(
                ctx,
                &r.path,
                r.commit,
                partition,
                r.timeout,
                r.chunk_delay_us,
                r.wait,
                p,
            )
            .map(|()| Some(r.commit))
        },
    )?;
    Ok(Json(Started { id }))
}

// --- card -------------------------------------------------------------------

#[derive(Deserialize)]
struct IndexWait {
    #[serde(default)]
    index: u16,
    #[serde(default = "wait_default")]
    wait: u64,
}

#[derive(Serialize)]
struct Size {
    width: u16,
    height: u16,
}

async fn get_screen_size(
    State(state): State<Shared>,
    Qs(q): Qs<IndexWait>,
) -> ApiResult<Json<Size>> {
    let ((width, height), _) = state
        .command("card screen-size", move |ctx, p| {
            screen::screen_size(ctx, None, false, q.index, q.wait, p)
        })
        .await?;
    Ok(Json(Size { width, height }))
}

#[derive(Deserialize)]
struct ScreenSizeReq {
    width: u16,
    height: u16,
    #[serde(default)]
    commit: bool,
    #[serde(default)]
    index: u16,
    #[serde(default = "wait_default")]
    wait: u64,
}

#[derive(Serialize)]
struct SizeOutcome {
    #[serde(flatten)]
    outcome: Outcome,
    width: u16,
    height: u16,
}

async fn put_screen_size(
    State(state): State<Shared>,
    Body(r): Body<ScreenSizeReq>,
) -> ApiResult<Json<SizeOutcome>> {
    let ((width, height), lines) = state
        .command("card screen-size", move |ctx, p| {
            screen::screen_size(ctx, Some((r.width, r.height)), r.commit, r.index, r.wait, p)
        })
        .await?;
    Ok(Json(SizeOutcome {
        outcome: Outcome {
            lines,
            files: Vec::new(),
            committed: Some(r.commit),
        },
        width,
        height,
    }))
}

#[derive(Deserialize)]
struct ReloadReq {
    #[serde(default)]
    index: u16,
    #[serde(default)]
    full: bool,
}

async fn card_reload(
    State(state): State<Shared>,
    Body(r): Body<ReloadReq>,
) -> ApiResult<Json<Outcome>> {
    let ((), lines) = state
        .command("card reload", move |ctx, _| {
            screen::reload(ctx, r.index, r.full)
        })
        .await?;
    Ok(outcome(lines, Vec::new(), None))
}

#[derive(Deserialize)]
struct TestModeReq {
    n: u8,
    #[serde(default)]
    index: u16,
}

async fn card_test_mode(
    State(state): State<Shared>,
    Body(r): Body<TestModeReq>,
) -> ApiResult<Json<Outcome>> {
    let ((), lines) = state
        .command("card test-mode", move |ctx, _| {
            screen::test_mode(ctx, r.index, r.n)
        })
        .await?;
    Ok(outcome(lines, Vec::new(), None))
}

#[derive(Deserialize)]
struct SetLayoutReq {
    panel_width: u16,
    panel_height: u16,
    #[serde(default)]
    index: u16,
}

async fn card_set_layout(
    State(state): State<Shared>,
    Body(r): Body<SetLayoutReq>,
) -> ApiResult<Json<Outcome>> {
    let ((), lines) = state
        .command("card set-layout", move |ctx, _| {
            screen::set_layout(ctx, r.index, r.panel_width, r.panel_height)
        })
        .await?;
    Ok(outcome(lines, Vec::new(), None))
}

// --- wall -------------------------------------------------------------------

async fn get_wall(State(state): State<Shared>) -> Json<Canvas> {
    Json(state.wall())
}

async fn put_wall(State(state): State<Shared>, Body(c): Body<Canvas>) -> ApiResult<Json<Canvas>> {
    c.validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    state
        .set_wall(c.clone())
        .map_err(|e| ApiError::command("wall", &e))?;
    Ok(Json(c))
}

// --- jobs -------------------------------------------------------------------

async fn list_jobs(State(state): State<Shared>) -> Json<Vec<Job>> {
    Json(state.jobs.list())
}

fn job(state: &AppState, id: &str) -> ApiResult<std::sync::Arc<crate::jobs::Handle>> {
    state
        .jobs
        .get(id)
        .ok_or_else(|| ApiError::not_found(format!("no job {id}")))
}

async fn get_job(State(state): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Job>> {
    Ok(Json(job(&state, &id)?.snapshot()))
}

async fn delete_job(State(state): State<Shared>, Path(id): Path<String>) -> ApiResult<Json<Job>> {
    let h = job(&state, &id)?;
    if h.is_running() {
        h.cancel();
        h.wait().await;
    }
    Ok(Json(h.snapshot()))
}

async fn job_events(State(state): State<Shared>, Path(id): Path<String>) -> ApiResult<Response> {
    Ok(job(&state, &id)?.events().into_response())
}
