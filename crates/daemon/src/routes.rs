//! One handler per route in `docs/ui.md` section 2.

use crate::api::{
    Brightness, Card, Cards, ConfigRead, ConfigReadReq, ConfigSendReq, ConfigWriteQuery,
    ConfigWriteReq, DiscoverReq, FirmwareCandidate, FirmwarePick, FirmwareReq, FirmwareUpload,
    FrameQuery, GenFileSet, GenFiles, Health, ProvisionReq, ReloadReq, RestoreReq, ScreenSizeQuery,
    ScreenSizeReq, SetLayoutReq, Settings, ShowFillReq, ShowImageReq, ShowKind, ShowPatternReq,
    ShowVideoReq, Size, SizeOutcome, SnapshotReq, SpecReq, Started, State as LiveState,
    TestModeReq, UploadQuery,
};
use crate::assets;
use crate::error::{ApiError, ApiResult};
use crate::jobs::{GatedOutcome, Job, JobKind, Line, Outcome};
use crate::state::{load_library, AppState};
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
use wall::Canvas;
use ops::{capture, config, display, flash, params, provision, restore, screen, upgrade};
use panelspec::PanelSpec;
use sources::Fit;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tower_http::cors::{Any, CorsLayer};

/// The whole application: the API under `/api/v1`, the web app elsewhere.
/// `GET /health` is added after the token layer, so it answers without one.
pub fn router(state: Shared) -> Router {
    let api = Router::new()
        .route("/discover", post(discover))
        .route("/settings", get(get_settings).put(put_settings))
        .route("/brightness", post(brightness))
        .route("/show/image", post(show_image))
        .route("/show/video", post(show_video))
        .route("/show/pattern", post(show_pattern))
        .route("/show/fill", post(show_fill))
        .route("/show/blank", post(show_blank))
        .route("/show/frame", post(show_frame))
        .route("/state", get(get_state))
        .route("/state/events", get(state_events))
        .route("/config/gen", post(config_gen))
        .route("/config/read", post(config_read))
        .route("/config/write", post(config_write))
        .route("/config/send", post(config_send))
        .route("/provision", post(provision_route))
        .route("/flash/snapshot", post(flash_snapshot))
        .route("/flash/restore", post(flash_restore))
        .route("/firmware/install", post(firmware_install))
        .route("/firmware/pick", post(firmware_pick))
        .route("/firmware/upload", post(firmware_upload))
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
        .layer(middleware::from_fn_with_state(state.clone(), token_check))
        .route("/health", get(health));
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

/// Every route behind this layer needs the daemon's token.
async fn token_check(State(state): State<Shared>, req: Request, next: Next) -> Response {
    if let Err(e) = authorize(&state, &req) {
        return e.into_response();
    }
    next.run(req).await
}

#[derive(Deserialize)]
struct TokenQs {
    token: Option<String>,
}

/// The token a request presents: the `X-Token` header, or `?token=` for
/// `EventSource`, which cannot set headers.
fn presented(req: &Request) -> Option<String> {
    if let Some(h) = req.headers().get("x-token") {
        return h.to_str().ok().map(str::to_owned);
    }
    Query::<TokenQs>::try_from_uri(req.uri())
        .ok()
        .and_then(|Query(q)| q.token)
}

/// 401 unless the request presents the daemon's token.
fn authorize(state: &AppState, req: &Request) -> ApiResult<()> {
    match presented(req) {
        None => Err(ApiError::unauthorized("token required")),
        Some(got) if !same(&got, &state.token) => Err(ApiError::unauthorized("bad token")),
        Some(_) => Ok(()),
    }
}

/// Equality whose time does not depend on where the strings differ.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
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

// --- shared ------------------------------------------------------------------

fn outcome(lines: Vec<Line>, files: Vec<String>) -> Outcome {
    Outcome { lines, files }
}

fn gated(lines: Vec<Line>, files: Vec<String>, committed: bool) -> GatedOutcome {
    GatedOutcome {
        outcome: outcome(lines, files),
        committed,
    }
}

/// Seconds a discovery-backed command waits when the request says nothing.
const WAIT: u64 = 3;

/// The request's content type, empty when it names none.
fn content_type(req: &Request) -> &str {
    req.headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

fn is_json(req: &Request) -> bool {
    content_type(req).starts_with("application/json")
}

fn is_multipart(req: &Request) -> bool {
    content_type(req).starts_with("multipart/form-data")
}

fn parse_fit(s: &str) -> ApiResult<Fit> {
    s.parse()
        .map_err(|e| ApiError::bad_request(format!("fit: {e}")))
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

async fn health(State(state): State<Shared>, req: Request) -> Json<Health> {
    let full = authorize(&state, &req).is_ok();
    Json(Health {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        iface: full.then(|| state.settings().iface),
        cards: full.then(|| lock(&state.cards).iter().map(Card::from).collect()),
    })
}

async fn discover(
    State(state): State<Shared>,
    Body(req): Body<DiscoverReq>,
) -> ApiResult<Json<Cards>> {
    let wait = req.wait.unwrap_or(WAIT);
    let (cards, _) = state
        .command("discover", move |ctx, p| capture::discover(ctx, wait, p))
        .await?;
    let out = cards.iter().map(Card::from).collect();
    state.set_cards(cards);
    Ok(Json(Cards { cards: out }))
}

// --- live state --------------------------------------------------------------

async fn get_state(State(state): State<Shared>) -> Json<LiveState> {
    Json(state.live.snapshot())
}

/// The same object as `GET /state`, at once and on every change.
async fn state_events(State(state): State<Shared>) -> Response {
    state.live.events().into_response()
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
    if let Some(name) = &s.card {
        ops::model::named(name).map_err(|e| ApiError::bad_request(format!("card: {e}")))?;
    }
    state
        .set_settings(s.clone())
        .map_err(|e| ApiError::command("settings", &e))?;
    Ok(Json(s))
}

async fn brightness(
    State(state): State<Shared>,
    Body(req): Body<Brightness>,
) -> ApiResult<Json<Brightness>> {
    let value = req.value;
    state
        .command("brightness", move |ctx, _| display::brightness(ctx, value))
        .await?;
    let mut s = state.settings();
    s.brightness = value;
    state
        .set_settings(s)
        .map_err(|e| ApiError::command("brightness", &e))?;
    state.live.set_brightness(value);
    Ok(Json(Brightness { value }))
}

// --- show -------------------------------------------------------------------

/// Three refreshes as a command, or a `show/hold` job; either replaces a
/// running show job.
async fn show_still(
    state: &Shared,
    subject: &'static str,
    canvas: Canvas,
    frame: wall::Frame,
    hold: bool,
    what: (ShowKind, String),
) -> ApiResult<Response> {
    state.cancel_show().await;
    let (kind, source) = what;
    if hold {
        let id = state.start_job(JobKind::ShowHold, subject, Vec::new(), move |ctx, p| {
            display::show_frame(ctx, canvas, &frame, true, p).map(|()| None)
        })?;
        state.showing(kind, source, None, Some(id.clone()));
        return Ok(Json(Started { id }).into_response());
    }
    let ((), lines) = state
        .command(subject, move |ctx, p| {
            display::show_frame(ctx, canvas, &frame, false, p)
        })
        .await?;
    state.showing(kind, source, None, None);
    Ok(Json(outcome(lines, Vec::new())).into_response())
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
    let (img, name, fit, hold) = if is_multipart(&req) {
        let mp = Multipart::from_request(req, &state)
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?;
        let (bytes, name, fit, hold) = image_upload(mp).await?;
        let img = image::load_from_memory(&bytes)
            .with_context(|| format!("decode image {name}"))
            .map_err(|e| ApiError::command("show image", &e))?;
        (img, name, fit, hold)
    } else {
        let Body(r) = Body::<ShowImageReq>::from_request(req, &state).await?;
        let img = image::open(&r.path)
            .with_context(|| format!("open image {}", r.path))
            .map_err(|e| ApiError::command("show image", &e))?;
        (img, r.path, r.fit.unwrap_or_default(), r.hold.unwrap_or(false))
    };
    let canvas = state.wall();
    let frame = display::image_frame(&img, &canvas, fit)
        .map_err(|e| ApiError::command("show image", &e))?;
    show_still(
        &state,
        "show image",
        canvas,
        frame,
        hold,
        (ShowKind::Still, name),
    )
    .await
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
    let fps = req.fps.unwrap_or(30);
    let fit = req.fit.unwrap_or(Fit::Contain);
    let looping = req.looping.unwrap_or(false);
    let source = req.path.clone();
    let id = state.start_job(JobKind::ShowVideo, "show video", Vec::new(), move |ctx, p| {
        display::play_on(ctx, canvas, &req.path, fps, fit, looping, p).map(|()| None)
    })?;
    state.showing(ShowKind::Video, source, Some(fps), Some(id.clone()));
    Ok(Json(Started { id }))
}

/// One rgb24 frame from a client that mirrors something onto the wall: the
/// 12-byte header `rxp show serve` reads, then `width * height * 3` bytes.
/// The first frame starts the `show/stream` job that owns the link; the
/// stream ends when the frames stop, when it is cancelled, or when another
/// show replaces it.
async fn show_frame(
    State(state): State<Shared>,
    Qs(q): Qs<FrameQuery>,
    body: Bytes,
) -> ApiResult<Json<Started>> {
    let head: [u8; sources::raw::Header::LEN] = body
        .get(..sources::raw::Header::LEN)
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| {
            ApiError::bad_request(format!(
                "frame: {} bytes, shorter than the {}-byte header",
                body.len(),
                sources::raw::Header::LEN
            ))
        })?;
    let header = sources::raw::Header::from_bytes(&head)
        .map_err(|e| ApiError::bad_request(format!("frame: {e}")))?;
    let want = sources::raw::Header::LEN + header.frame_len();
    if body.len() != want {
        return Err(ApiError::bad_request(format!(
            "frame: {} bytes, header says {}x{} rgb24 is {want}",
            body.len(),
            header.width,
            header.height
        )));
    }
    let frame = wall::Frame::from_rgb(
        u32::from(header.width),
        u32::from(header.height),
        body[sources::raw::Header::LEN..].to_vec(),
    )
    .map_err(|e| ApiError::bad_request(format!("frame: {e:?}")))?;
    let source = q.source.unwrap_or_else(|| "frame push".to_owned());
    let id = state
        .push_frame(header, frame, source, q.fit.unwrap_or(Fit::Contain))
        .await?;
    Ok(Json(Started { id }))
}

async fn show_pattern(
    State(state): State<Shared>,
    Body(req): Body<ShowPatternReq>,
) -> ApiResult<Response> {
    let canvas = state.wall();
    let frame = sources::pattern(req.name, canvas.width, canvas.height);
    show_still(
        &state,
        "show pattern",
        canvas,
        frame,
        req.hold.unwrap_or(false),
        (ShowKind::Pattern, req.name.to_string()),
    )
    .await
}

async fn show_fill(
    State(state): State<Shared>,
    Body(req): Body<ShowFillReq>,
) -> ApiResult<Response> {
    let rgb = ops::util::parse_color(std::slice::from_ref(&req.rgb))
        .map_err(|e| ApiError::bad_request(format!("rgb: {e:#}")))?;
    let canvas = state.wall();
    let frame = solid(&canvas, rgb)?;
    let source = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
    show_still(
        &state,
        "show fill",
        canvas,
        frame,
        req.hold.unwrap_or(false),
        (ShowKind::Still, source),
    )
    .await
}

fn solid(canvas: &Canvas, rgb: [u8; 3]) -> ApiResult<wall::Frame> {
    wall::Frame::from_rgb(
        canvas.width,
        canvas.height,
        rgb.repeat((canvas.width * canvas.height) as usize),
    )
    .map_err(|e| ApiError::command("show fill", &anyhow::anyhow!("{e:?}")))
}

async fn show_blank(State(state): State<Shared>) -> ApiResult<Response> {
    let canvas = state.wall();
    let frame = solid(&canvas, [0, 0, 0])?;
    show_still(
        &state,
        "show blank",
        canvas,
        frame,
        false,
        (ShowKind::Blank, "blank".to_owned()),
    )
    .await
}

// --- config -----------------------------------------------------------------

fn parse_spec(subject: &str, toml: &str) -> ApiResult<PanelSpec> {
    PanelSpec::parse(toml)
        .context("parse spec")
        .map_err(|e| ApiError::command(subject, &e))
}

async fn config_gen(State(state): State<Shared>, Body(req): Body<SpecReq>) -> ApiResult<Json<GenFiles>> {
    let spec = parse_spec("config gen", &req.spec_toml)?;
    let label = format!("{}.toml", spec.name);
    // No hardware: the boot image is laid out for the settings' card, the
    // discovered one, else the first tested model.
    let card = state.ctx().model.unwrap_or_else(receivers::default_model);
    let g = config::generate(card, &spec, &label, "rcvbp", &load_library)
        .map_err(|e| ApiError::command("config gen", &e))?;
    Ok(Json(GenFiles {
        name: g.name,
        files: GenFileSet {
            rcvbp: BASE64_STANDARD.encode(&g.rcvbp),
            basic_pack: BASE64_STANDARD.encode(&g.basic_pack),
            block7: g.block7.as_deref().map(|b| BASE64_STANDARD.encode(b)),
            sources_txt: g.report,
        },
        sources: g.sources,
        notes: g.notes,
    }))
}

async fn config_read(
    State(state): State<Shared>,
    Body(r): Body<ConfigReadReq>,
) -> ApiResult<Json<ConfigRead>> {
    let (bytes, lines) = state
        .command("config read", move |ctx, _| {
            flash::read_config(
                ctx,
                r.index.unwrap_or(0),
                r.page,
                r.max_chunks.unwrap_or(64),
                r.wait.unwrap_or(2),
            )
        })
        .await?;
    Ok(Json(ConfigRead {
        rcvbp: BASE64_STANDARD.encode(&bytes),
        lines,
    }))
}

/// The `.rcvbp` bytes and the gate: a JSON body carries them base64, any
/// other content type is the file itself with the gate in the query.
async fn write_body(state: &Shared, req: Request) -> ApiResult<(Vec<u8>, ConfigWriteReq)> {
    if is_json(&req) {
        let Body(r) = Body::<ConfigWriteReq>::from_request(req, state).await?;
        let bytes = BASE64_STANDARD
            .decode(&r.rcvbp)
            .map_err(|e| ApiError::bad_request(format!("rcvbp: {e}")))?;
        return Ok((bytes, r));
    }
    let (mut parts, body) = req.into_parts();
    let Qs(q) = Qs::<ConfigWriteQuery>::from_request_parts(&mut parts, state).await?;
    let bytes = Bytes::from_request(Request::from_parts(parts, body), state)
        .await
        .map_err(|e| ApiError::bad_request(e.body_text()))?;
    if bytes.is_empty() {
        return Err(ApiError::bad_request("rcvbp: empty body"));
    }
    Ok((
        bytes.to_vec(),
        ConfigWriteReq {
            rcvbp: String::new(),
            commit: q.commit,
            index: q.index,
            wait: q.wait,
        },
    ))
}

async fn config_write(
    State(state): State<Shared>,
    req: Request,
) -> ApiResult<Json<GatedOutcome>> {
    let (bytes, r) = write_body(&state, req).await?;
    let ts = stamp();
    let config = data_path(&state, "backups", &format!("config-{ts}.rcvbp"))?;
    std::fs::write(&config, &bytes)
        .map_err(|e| ApiError::command("config write", &anyhow::anyhow!("write {config}: {e}")))?;
    let backup = data_path(&state, "backups", &format!("block07-{ts}.bin"))?;
    let files = vec![config.clone(), backup.clone()];
    let commit = r.commit.unwrap_or(false);
    let ((), lines) = state
        .command("config write", move |ctx, p| {
            flash::write_config(
                ctx,
                &config,
                commit,
                &backup,
                None,
                r.index.unwrap_or(0),
                r.wait.unwrap_or(2),
                p,
            )
        })
        .await?;
    Ok(Json(gated(lines, files, commit)))
}

async fn config_send(
    State(state): State<Shared>,
    Body(r): Body<ConfigSendReq>,
) -> ApiResult<Json<Outcome>> {
    let spec = parse_spec("config send", &r.spec_toml)?;
    let g = spec
        .chip_library(&load_library)
        .and_then(|chip| rcvbp::spec::generate(&spec, &chip))
        .map_err(|e| ApiError::command("config send", &e))?;
    let ((), lines) = state
        .command("config send", move |ctx, _| {
            params::send_generated(
                ctx,
                &spec,
                &g,
                r.chip_only.unwrap_or(false),
                r.gap_ms.unwrap_or(8),
            )
        })
        .await?;
    Ok(Json(outcome(lines, Vec::new())))
}

// --- provision, flash, firmware ---------------------------------------------

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
    let commit = r.commit.unwrap_or(false);
    let id = state.start_job(JobKind::Provision, "provision", files, move |ctx, p| {
        provision::provision(
            ctx,
            &provision::Args {
                spec_path: &spec_path,
                firmware: r.firmware_path.as_deref(),
                position: r.position,
                index: r.index,
                snapshot_dir: Some(&dir),
                commit,
                wait: r.wait.unwrap_or(WAIT),
            },
            &load_library,
            p,
        )
        .map(|()| Some(commit))
    })?;
    Ok(Json(Started { id }))
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
    let id = state.start_job(JobKind::FlashSnapshot, "flash snapshot", files, move |ctx, p| {
        restore::snapshot(ctx, &dir, r.index.unwrap_or(0), r.wait.unwrap_or(WAIT), p)
            .map(|()| None)
    })?;
    Ok(Json(Started { id }))
}

async fn flash_restore(
    State(state): State<Shared>,
    Body(r): Body<RestoreReq>,
) -> ApiResult<Json<Started>> {
    let commit = r.commit.unwrap_or(false);
    let id = state.start_job(
        JobKind::FlashRestore,
        "flash restore",
        Vec::new(),
        move |ctx, p| {
            restore::all(
                ctx,
                &r.dir,
                commit,
                r.index.unwrap_or(0),
                r.wait.unwrap_or(WAIT),
                p,
            )
            .map(|()| Some(commit))
        },
    )?;
    Ok(Json(Started { id }))
}

async fn firmware_install(
    State(state): State<Shared>,
    Body(r): Body<FirmwareReq>,
) -> ApiResult<Json<Started>> {
    let partition = if r.golden.unwrap_or(false) {
        colorlight::upgrade::Partition::Golden
    } else {
        colorlight::upgrade::Partition::Primary
    };
    let commit = r.commit.unwrap_or(false);
    let id = state.start_job(
        JobKind::FirmwareInstall,
        "firmware install",
        Vec::new(),
        move |ctx, p| {
            upgrade::install(
                ctx,
                &r.path,
                commit,
                partition,
                r.timeout.unwrap_or(120),
                r.chunk_delay_us.unwrap_or(3000),
                r.wait.unwrap_or(4),
                p,
            )
            .map(|()| Some(commit))
        },
    )?;
    Ok(Json(Started { id }))
}

/// A multipart `firmware/upload`: the `file` part's bytes and name.
async fn firmware_part(mut mp: Multipart) -> ApiResult<(Bytes, String)> {
    let bad = |e: axum::extract::multipart::MultipartError| ApiError::bad_request(e.body_text());
    while let Some(field) = mp.next_field().await.map_err(bad)? {
        if field.name() == Some("file") {
            let name = field.file_name().unwrap_or("upload.hex").to_owned();
            return Ok((field.bytes().await.map_err(bad)?, name));
        }
    }
    Err(ApiError::bad_request("multipart: no file part"))
}

/// Take a firmware image the browser holds and put it where
/// `POST /firmware/install` can read it: `<data dir>/firmware/<name>`.
///
/// A name `config/firmware.toml` lists is checked against that entry and
/// refused when the size or the sha256 disagrees; any other name is kept as
/// it is, with its hash reported and `verified: false`.
async fn firmware_upload(State(state): State<Shared>, req: Request) -> ApiResult<Json<FirmwareUpload>> {
    let (bytes, name) = if is_multipart(&req) {
        let mp = Multipart::from_request(req, &state)
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?;
        firmware_part(mp).await?
    } else {
        let (mut parts, body) = req.into_parts();
        let Qs(q) = Qs::<UploadQuery>::from_request_parts(&mut parts, &state).await?;
        let bytes = Bytes::from_request(Request::from_parts(parts, body), &state)
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?;
        (bytes, q.name.unwrap_or_else(|| "upload.hex".to_owned()))
    };
    // The name becomes a file name: a path from the client is not one.
    let name = std::path::Path::new(&name)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| ApiError::bad_request(format!("firmware upload: {name} is not a file name")))?
        .to_owned();
    let sha256 = receivers::firmware::sha256_hex(&bytes);
    let entry = receivers::firmware::image(&name);
    if let Some(image) = entry {
        image
            .verify(&bytes)
            .map_err(|e| ApiError::bad_request(format!("firmware upload: {e}")))?;
    }
    let path = data_path(&state, "firmware", &name)?;
    std::fs::write(&path, &bytes).map_err(|e| {
        ApiError::command("firmware upload", &anyhow::anyhow!("write {path}: {e}"))
    })?;
    Ok(Json(FirmwareUpload {
        name,
        path,
        size: bytes.len() as u64,
        sha256,
        verified: entry.is_some(),
        manifest_sha256: entry.map(|i| i.sha256.clone()),
    }))
}

/// The manifest ranked for a spec: offline, no link, no card needed. The
/// card model is the `card` setting or the last discovered one, else the
/// first tested model.
async fn firmware_pick(
    State(state): State<Shared>,
    Body(r): Body<SpecReq>,
) -> ApiResult<Json<FirmwarePick>> {
    let spec = parse_spec("firmware pick", &r.spec_toml)?;
    let card = state.ctx().model.unwrap_or_else(receivers::default_model);
    let ranked = ops::firmware::select(&spec, card);
    let chip = ops::firmware::chip_name(&spec);
    let decided = ops::firmware::chosen(&ranked, &chip);
    Ok(Json(FirmwarePick {
        chosen: decided.as_ref().ok().map(|c| c.image.name.clone()),
        refused: decided.err().map(|e| format!("{e:#}")),
        chip,
        card: card.name.clone(),
        candidates: ranked
            .iter()
            .map(|c| FirmwareCandidate {
                name: c.image.name.clone(),
                version: c.image.version.to_string(),
                pcb: c.image.pcb.clone(),
                kind: c.image.kind.clone(),
                chips: c.image.chips.clone(),
                size: receivers::firmware::manifest().size,
                sha256: c.image.sha256.clone(),
                score: c.score,
                reasons: c.reasons.clone(),
            })
            .collect(),
    }))
}

// --- card -------------------------------------------------------------------

async fn get_screen_size(
    State(state): State<Shared>,
    Qs(q): Qs<ScreenSizeQuery>,
) -> ApiResult<Json<Size>> {
    let ((width, height), _) = state
        .command("card screen-size", move |ctx, p| {
            screen::screen_size(
                ctx,
                None,
                false,
                q.index.unwrap_or(0),
                q.wait.unwrap_or(WAIT),
                p,
            )
        })
        .await?;
    Ok(Json(Size { width, height }))
}

async fn put_screen_size(
    State(state): State<Shared>,
    Body(r): Body<ScreenSizeReq>,
) -> ApiResult<Json<SizeOutcome>> {
    let commit = r.commit.unwrap_or(false);
    let ((width, height), lines) = state
        .command("card screen-size", move |ctx, p| {
            screen::screen_size(
                ctx,
                Some((r.width, r.height)),
                commit,
                r.index.unwrap_or(0),
                r.wait.unwrap_or(WAIT),
                p,
            )
        })
        .await?;
    Ok(Json(SizeOutcome {
        outcome: gated(lines, Vec::new(), commit),
        width,
        height,
    }))
}

async fn card_reload(
    State(state): State<Shared>,
    Body(r): Body<ReloadReq>,
) -> ApiResult<Json<Outcome>> {
    let ((), lines) = state
        .command("card reload", move |ctx, _| {
            screen::reload(ctx, r.index.unwrap_or(0), r.full.unwrap_or(false))
        })
        .await?;
    Ok(Json(outcome(lines, Vec::new())))
}

async fn card_test_mode(
    State(state): State<Shared>,
    Body(r): Body<TestModeReq>,
) -> ApiResult<Json<Outcome>> {
    let ((), lines) = state
        .command("card test-mode", move |ctx, _| {
            screen::test_mode(ctx, r.index.unwrap_or(0), r.n)
        })
        .await?;
    Ok(Json(outcome(lines, Vec::new())))
}

async fn card_set_layout(
    State(state): State<Shared>,
    Body(r): Body<SetLayoutReq>,
) -> ApiResult<Json<Outcome>> {
    let ((), lines) = state
        .command("card set-layout", move |ctx, _| {
            screen::set_layout(ctx, r.index.unwrap_or(0), r.panel_width, r.panel_height)
        })
        .await?;
    Ok(Json(outcome(lines, Vec::new())))
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
