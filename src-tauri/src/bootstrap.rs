//! Drives the *shared* (not per-instance) Podman runtime's first-run/repair
//! bootstrap, and owns the 4-command IPC surface the isolated "onboarding"
//! window uses to run it — modeled on `omnideck/desktop` (sibling repo)'s
//! setup flow, adapted per `reference/desktop-hardening-migration-PLAN.md`'s
//! Phase 5.
//!
//! Correcting this repo's earlier assumption (see AGENT.md): the CLI does
//! have an equivalent for "detect/install podman, WSL2, podman machine" as
//! of `v0.10.0` — `runtime status`/`runtime ensure` (`cmd/runtime.go` in the
//! CLI repo). This module is a thin driver over that NDJSON stream
//! ([`cli_bridge::runtime_ensure`]), translating its `stage`/`activity`/
//! error-code vocabulary into [`SetupState`] — it does not reimplement any
//! platform-specific installer logic itself.
//!
//! Scope boundary: this module's job ends once the shared runtime is ready.
//! Creating a Deck (pulling the omnideck image, provisioning a container) is
//! a separate, already-built flow (`NewDeckForm.tsx` → `add_instance`) that
//! runs from the dashboard once it's shown — not part of bootstrap at all.
//!
//! No resume-record file (contrast the sibling's `setup-state.json`): unlike
//! the sibling, there's no per-launch "did we finish pulling image X into
//! container Y" state to remember, because there's no image/container step
//! here — `runtime status` is cheap and authoritative on every launch, so
//! querying it fresh each time *is* the resume mechanism. A crash mid
//! `runtime ensure` just means the next launch's `runtime status` still
//! reports not-ready and drives setup again, which is the CLI's own job to
//! make idempotent (same idempotency contract as `environment ensure`,
//! JSON_MODE_SPEC's "Desktop runtime contract").
//!
//! **This two-window design survived a real scare, worth knowing about**:
//! mid-development this repo briefly went single-window (onboarding as a
//! React screen `App.tsx` swapped in, no separate window), suspecting the
//! two-GTK/WebKit-windows-at-startup pattern caused a real
//! `EGL_BAD_PARAMETER` crash on some Linux hardware. It didn't — confirmed
//! over two more rounds of real-hardware testing (first with the single
//! window, then also with `tauri::ipc::Channel`/`WebviewWindow` replaced by
//! `app.emit()`/`AppHandle`, on the theory those were unique-to-this-module
//! mechanisms — still crashed both times). The actual cause: CI built the
//! Linux release AppImage directly on `ubuntu-24.04`, and `linuxdeploy`
//! bundles whatever GTK/WebKit/libepoxy libraries exist on the *build
//! machine* — Ubuntu's crashed on the affected hardware, this repo's own
//! Fedora dev toolbox's didn't. Fixed at the CI level (`release.yml`'s
//! `linux-x64` matrix now builds inside `container: fedora:42`), nothing to
//! do with window count or IPC mechanism. Full account in `AGENT.md`'s "The
//! `EGL_BAD_PARAMETER` AppImage crash" section — if this bug class ever
//! resurfaces, check the build container before re-litigating this module.

use crate::cli_bridge::{self, CliError, RuntimeSetupEvent};
use serde::Serialize;
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
};
use tauri::{ipc::Channel, AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Phases in weighted-progress order, matching `runtime ensure`'s own stage
/// vocabulary exactly (`engine.SetupStageSoftware`/`SetupStageEnvironment`
/// in the CLI repo) — there is no third/fourth phase here the way the
/// sibling has "download"/"startup", because pulling an image and creating
/// a Deck isn't part of runtime bootstrap in this repo's multi-instance
/// model.
const PHASES: &[(&str, &str, f64)] = &[
    ("software", "Installing Podman", 0.4),
    ("environment", "Preparing a secure space", 0.6),
];

fn phase_index(id: &str) -> Option<usize> {
    PHASES.iter().position(|(phase_id, ..)| *phase_id == id)
}

fn overall_progress(index: usize, fraction: f64) -> f64 {
    let total: f64 = PHASES.iter().map(|(.., weight)| weight).sum();
    let done: f64 = PHASES.iter().take(index).map(|(.., weight)| weight).sum();
    let current = PHASES[index].2 * fraction.clamp(0.0, 1.0);
    ((done + current) / total).clamp(0.0, 1.0)
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub id: String,
    pub label: String,
    /// `"pass" | "issue" | "waiting"`.
    pub status: String,
}

/// Pushed from Rust to the onboarding webview over a Tauri [`Channel`]. One
/// `render(state)`-style function on the JS side fully re-derives the DOM
/// from each pushed state — see `public/onboarding/setup.js`. Modeled on the
/// sibling's `SetupState` in `parity.rs`.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    /// `"welcome" | "preparing" | "ready" | "error"`.
    pub stage: String,
    pub title: String,
    pub detail: String,
    pub progress: Option<f64>,
    pub indeterminate: bool,
    pub can_start: bool,
    pub can_retry: bool,
    pub can_open: bool,
    pub activity: Option<String>,
    /// Short secondary label alongside `activity` (e.g. `"Password
    /// required"`, `"Package manager running"`) — new in CLI contract `3`'s
    /// `runtime ensure` events (`RuntimeSetupEvent::status`), distinct from
    /// the longer `activity` headline. `None` outside `preparing`.
    pub status: Option<String>,
    /// Stable machine-readable step id (e.g. `"wsl-permission"`,
    /// `"package-index"`) — not currently rendered by `setup.js` (this
    /// app's diagnostics are still phase-level, not sub-step-level), but
    /// captured and pushed through so it shows up in bug reports/devtools
    /// without another round of plumbing later.
    pub substage: Option<String>,
    /// True when the CLI is waiting on a native OS permission/security
    /// prompt (`RuntimeSetupEvent::state == "permission"`), not doing
    /// background work — lets the UI follow the sibling's setup-UX
    /// principle of keeping native prompts visible and using truthful
    /// "waiting for you" copy instead of a synthesized percentage.
    pub awaiting_permission: bool,
    pub primary_action: Option<String>,
    pub primary_label: Option<String>,
    pub secondary_action: Option<String>,
    pub secondary_label: Option<String>,
    pub diagnostics: Option<Vec<Diagnostic>>,
    pub technical: Option<String>,
}

fn base_state(stage: &str, title: &str, detail: &str) -> SetupState {
    SetupState {
        stage: stage.to_owned(),
        title: title.to_owned(),
        detail: detail.to_owned(),
        progress: None,
        indeterminate: false,
        can_start: false,
        can_retry: false,
        can_open: false,
        activity: None,
        status: None,
        substage: None,
        awaiting_permission: false,
        primary_action: None,
        primary_label: None,
        secondary_action: None,
        secondary_label: None,
        diagnostics: None,
        technical: None,
    }
}

fn welcome_state() -> SetupState {
    let mut state = base_state(
        "welcome",
        "Let's get Omnideck ready",
        "Omnideck runs your agents in an isolated Podman environment. This only needs to happen once.",
    );
    state.can_start = true;
    state
}

/// `activity` prefers the CLI's own copy (`RuntimeSetupEvent::activity` —
/// `engine.SetupActivitySoftware`/`SetupActivityEnvironment` in the CLI
/// repo) over this module's own `PHASES` labels, which only exist as a
/// fallback for the moment before the first real event arrives. The CLI's
/// copy is authoritative and already user-tested; duplicating it here would
/// just be a second place for the two to drift. `awaiting_permission`
/// forces indeterminate progress even if a fraction is known — a percentage
/// is meaningless while genuinely waiting on the user, not the computer.
fn preparing_state(
    phase: Option<usize>,
    fraction: f64,
    activity: Option<String>,
    status: Option<String>,
    substage: Option<String>,
    awaiting_permission: bool,
) -> SetupState {
    let mut state = base_state(
        "preparing",
        "Setting things up",
        "This can take a few minutes the first time.",
    );
    state.progress = phase.map(|index| overall_progress(index, fraction));
    state.indeterminate = phase.is_none() || awaiting_permission;
    state.activity = activity.or_else(|| phase.map(|index| PHASES[index].1.to_owned()));
    state.status = status;
    state.substage = substage;
    state.awaiting_permission = awaiting_permission;
    state
}

fn ready_state() -> SetupState {
    let mut state = base_state(
        "ready",
        "Omnideck is ready",
        "Everything is prepared. Continue to your Decks.",
    );
    state.progress = Some(1.0);
    state.can_open = true;
    state
}

/// One of a small, fixed set of user-facing failure kinds, each with its own
/// copy — never a raw error/stack trace. Built directly from the CLI's own
/// closed error-code vocabulary for `runtime ensure` (`RESTART_REQUIRED`,
/// `PERMISSION_DENIED`, `DOWNLOAD_FAILED`, `UNSUPPORTED`, and the catch-all
/// `RUNTIME_SETUP_FAILED` — confirmed against `cmd/runtime.go`, not
/// invented), plus this app's own process/contract failure modes. Smaller
/// than the sibling's 9-kind taxonomy because this repo's actual observed
/// error surface for *runtime* bootstrap specifically is smaller — no
/// per-instance kinds like `PORT_IN_USE`, which only apply to `add`/
/// `environment ensure`, not `runtime ensure`.
fn error_state(error: &CliError, reached_phase: usize) -> SetupState {
    let (title, detail, can_retry, primary_label): (&str, String, bool, &str) = match error {
        CliError::Cli(body) => match body.code.as_str() {
            "RESTART_REQUIRED" => (
                "Restart needed",
                body.message.clone(),
                false,
                "Quit Omnideck",
            ),
            "PERMISSION_DENIED" => (
                "Permission needed",
                body.message.clone(),
                true,
                "Try again",
            ),
            "DOWNLOAD_FAILED" => (
                "Download failed",
                "Check your internet connection, then try again.".to_owned(),
                true,
                "Try again",
            ),
            "UNSUPPORTED" => (
                "This computer isn't supported",
                body.message.clone(),
                false,
                "Quit Omnideck",
            ),
            // Four codes new in CLI contract `3` (`v0.11.0-alpha.2`) —
            // confirmed against `contracts/json/v3/error.schema.json` and
            // `engine/runtime_setup_{linux,macos,windows}_host.go`'s real
            // call sites, not invented. All four are retryable: the CLI's
            // own `message`/`hint` text for each already tells the user
            // "try again" (after restarting, for `WINDOWS_FEATURES_FAILED`)
            // rather than treating any of them as terminal.
            "PERMISSION_CANCELLED" => (
                "Permission not granted",
                body.message.clone(),
                true,
                "Try again",
            ),
            "WINDOWS_FEATURES_FAILED" => (
                "Windows setup couldn't finish",
                body.message.clone(),
                true,
                "Try again",
            ),
            "PACKAGE_INDEX_FAILED" => (
                "Couldn't check available software",
                body.message.clone(),
                true,
                "Try again",
            ),
            "INSTALLER_FAILED" => (
                "Installation failed",
                body.message.clone(),
                true,
                "Try again",
            ),
            _ => (
                "Setup needs attention",
                body.message.clone(),
                true,
                "Try again",
            ),
        },
        CliError::VersionTooOld { minimum, actual } => (
            "Omnideck needs an update",
            format!("This app requires omnideck {minimum} or newer, but found {actual}. Please reinstall Omnideck."),
            false,
            "Quit Omnideck",
        ),
        CliError::ContractMismatch { .. } | CliError::RuntimeSchemaMismatch { .. } => (
            "Omnideck needs an update",
            "This app build doesn't match the bundled Omnideck CLI. Please reinstall Omnideck.".to_owned(),
            false,
            "Quit Omnideck",
        ),
        _ => (
            "Setup needs attention",
            error.to_string(),
            true,
            "Try again",
        ),
    };

    let mut state = base_state("error", title, &detail);
    state.can_retry = can_retry;
    state.primary_action = Some(if can_retry { "retry" } else { "quit" }.to_owned());
    state.primary_label = Some(primary_label.to_owned());
    state.secondary_action = Some("quit".to_owned());
    state.secondary_label = Some("Quit".to_owned());
    state.technical = Some(error.to_string().chars().take(4_000).collect());
    state.diagnostics = Some(
        PHASES
            .iter()
            .enumerate()
            .map(|(index, (id, label, _))| {
                let status = match index.cmp(&reached_phase) {
                    std::cmp::Ordering::Less => "pass",
                    std::cmp::Ordering::Equal => "issue",
                    std::cmp::Ordering::Greater => "waiting",
                };
                Diagnostic {
                    id: (*id).to_owned(),
                    label: (*label).to_owned(),
                    status: status.to_owned(),
                }
            })
            .collect(),
    );
    state
}

/// Every failure mode of the 4 onboarding commands themselves — distinct
/// from [`CliError`] (which is specifically about CLI subprocess failures),
/// since these also cover the window-authorization and action-allowlist
/// checks that have nothing to do with the CLI.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BootstrapError {
    /// The caller wasn't the isolated "onboarding" window — the whole point
    /// of Phase 2's capability split is that this bridge isn't reachable
    /// from the dashboard or any instance webview.
    OriginDenied,
    Cli(CliError),
    /// The requested `run_action`/`open_dashboard` wasn't in the *current*
    /// state's offered actions (or, for `open_dashboard`, setup isn't
    /// actually done yet) — stops a compromised/buggy webview invoking
    /// something the current state never offered.
    ActionDenied,
    WindowMissing,
    StateLockPoisoned,
    StateDeliveryFailed,
}

impl From<CliError> for BootstrapError {
    fn from(error: CliError) -> Self {
        BootstrapError::Cli(error)
    }
}

/// `true` for the app's own served local content — the schemes/hosts Tauri
/// itself serves `public/onboarding/index.html` under, never remote
/// content. Matches the sibling's `is_local_setup_url` (`lib.rs`) exactly.
fn is_local_setup_url(url: &tauri::Url) -> bool {
    matches!(
        (url.scheme(), url.host_str()),
        ("tauri", Some("localhost"))
            | ("http", Some("tauri.localhost"))
            | ("https", Some("tauri.localhost"))
    )
}

/// Checks both the window label *and* the URL it actually has loaded —
/// label alone isn't quite enough in principle (a compromised or buggy
/// navigation could point the `"onboarding"` window at something else
/// first), so this also confirms it's still this app's own served content.
/// Ported from the sibling's `authorize_local_setup`.
fn authorize_onboarding(window: &WebviewWindow) -> Result<(), BootstrapError> {
    let url = window.url().map_err(|_| BootstrapError::OriginDenied)?;
    if window.label() != "onboarding" || !is_local_setup_url(&url) {
        return Err(BootstrapError::OriginDenied);
    }
    Ok(())
}

#[derive(Clone)]
pub struct BootstrapState {
    setup_running: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
    offered_actions: Arc<RwLock<HashSet<String>>>,
}

impl Default for BootstrapState {
    fn default() -> Self {
        Self {
            setup_running: Arc::new(AtomicBool::new(false)),
            ready: Arc::new(AtomicBool::new(false)),
            offered_actions: Arc::new(RwLock::new(HashSet::new())),
        }
    }
}

fn send_state(
    state: &BootstrapState,
    channel: &Channel<SetupState>,
    setup_state: SetupState,
) -> Result<(), BootstrapError> {
    state.ready.store(setup_state.can_open, Ordering::Release);
    let mut actions = state
        .offered_actions
        .write()
        .map_err(|_| BootstrapError::StateLockPoisoned)?;
    actions.clear();
    actions.extend(setup_state.primary_action.iter().cloned());
    actions.extend(setup_state.secondary_action.iter().cloned());
    drop(actions);
    channel
        .send(setup_state)
        .map_err(|_| BootstrapError::StateDeliveryFailed)
}

/// Lets a developer preview any onboarding screen on demand, without
/// touching real Podman state — set `OMNIDECK_DEBUG_ONBOARDING_STAGE` to
/// `welcome`/`preparing`/`permission`/`ready`/`error` and launch
/// `npm run dev:app`. See the README's "Testing the onboarding flow"
/// section.
///
/// `#[cfg(debug_assertions)]`-gated, not just "off by default": this
/// function — and the env var read — doesn't exist at all in a release
/// build (`cargo build --release`/`tauri build` compile with
/// `debug_assertions` off), so there's no runtime flag to accidentally ship
/// enabled and no code path a packaged build could hit.
#[cfg(debug_assertions)]
fn debug_forced_state() -> Option<SetupState> {
    let stage = std::env::var("OMNIDECK_DEBUG_ONBOARDING_STAGE").ok()?;
    Some(match stage.as_str() {
        "welcome" => welcome_state(),
        "preparing" => preparing_state(
            phase_index("environment"),
            0.5,
            Some("Preparing a secure space to run in…".to_owned()),
            None,
            None,
            false,
        ),
        "permission" => preparing_state(
            phase_index("software"),
            0.0,
            Some("Waiting for approval from your computer…".to_owned()),
            Some("Password required".to_owned()),
            Some("linux-permission".to_owned()),
            true,
        ),
        "ready" => ready_state(),
        "error" => error_state(
            &CliError::Cli(Box::new(cli_bridge::CliErrorBody {
                code: "DOWNLOAD_FAILED".to_owned(),
                message: format!(
                    "Simulated failure — set by OMNIDECK_DEBUG_ONBOARDING_STAGE={stage}."
                ),
                hint: None,
                detail: None,
                action: None,
                action_value: None,
                instances: None,
            })),
            0,
        ),
        _ => return None,
    })
}

#[cfg(not(debug_assertions))]
fn debug_forced_state() -> Option<SetupState> {
    None
}

/// Checks the shared runtime once and reports whether onboarding needs to
/// run — called by the onboarding window's own script on load, mirroring
/// the sibling's `setup.js` calling `bootstrap` immediately. Never mutates
/// anything.
///
/// Only reveals the onboarding window when it's actually needed (not ready,
/// or the check itself failed) — the dashboard (`"main"`) is already
/// visible by default on every launch (AGENT.md's rule, unchanged by any of
/// this), so the ready case must do nothing further and leave onboarding
/// hidden. Getting this wrong would mean onboarding popping up on *every*
/// launch even when the runtime is already ready, which defeats the entire
/// point of checking first.
#[tauri::command]
pub async fn bootstrap(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<'_, BootstrapState>,
    on_event: Channel<SetupState>,
) -> Result<(), BootstrapError> {
    authorize_onboarding(&window)?;

    if let Some(forced) = debug_forced_state() {
        send_state(&state, &on_event, forced)?;
        show_onboarding(&app)?;
        return Ok(());
    }

    match cli_bridge::runtime_status(&app).await {
        Ok(status) if status.ready => {
            send_state(&state, &on_event, ready_state())?;
        }
        Ok(_) => {
            send_state(&state, &on_event, welcome_state())?;
            show_onboarding(&app)?;
        }
        Err(error) => {
            send_state(&state, &on_event, error_state(&error, 0))?;
            show_onboarding(&app)?;
        }
    }
    Ok(())
}

/// Drives `runtime ensure`, streaming progress into the onboarding window
/// until the shared runtime is ready or setup fails. Re-entrant calls while
/// already running are ignored (matches the sibling's `setup_running` swap).
#[tauri::command]
pub async fn begin_setup(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<'_, BootstrapState>,
    on_event: Channel<SetupState>,
) -> Result<(), BootstrapError> {
    authorize_onboarding(&window)?;
    if state.setup_running.swap(true, Ordering::AcqRel) {
        return Ok(());
    }

    send_state(
        &state,
        &on_event,
        preparing_state(None, 0.0, None, None, None, false),
    )?;

    let progress_channel = on_event.clone();
    let progress_state = state.inner().clone();
    let result = cli_bridge::runtime_ensure(&app, move |event: RuntimeSetupEvent| {
        let index = phase_index(&event.stage);
        let fraction = event.progress.unwrap_or(0.0);
        let awaiting_permission = event.state.as_deref() == Some("permission");
        // Prefer the CLI's `detail` when present — it's the more specific of
        // the two ("Downloading Podman for macOS…" vs. just "Getting your
        // computer ready…") — falling back to `activity`.
        let activity = event.detail.or(event.activity);
        let _ = send_state(
            &progress_state,
            &progress_channel,
            preparing_state(
                index,
                fraction,
                activity,
                event.status,
                event.substage,
                awaiting_permission,
            ),
        );
    })
    .await;

    state.setup_running.store(false, Ordering::Release);

    match result {
        Ok(status) if status.ready => {
            send_state(&state, &on_event, ready_state())?;
        }
        Ok(status) => {
            let error = CliError::Cli(Box::new(cli_bridge::CliErrorBody {
                code: "RUNTIME_SETUP_FAILED".to_owned(),
                message: format!(
                    "Podman setup finished, but the runtime is still not ready ({}).",
                    status.state
                ),
                hint: None,
                detail: None,
                action: None,
                action_value: None,
                instances: None,
            }));
            send_state(&state, &on_event, error_state(&error, PHASES.len()))?;
        }
        Err(error) => {
            let reached = phase_index("environment").unwrap_or(PHASES.len() - 1);
            send_state(&state, &on_event, error_state(&error, reached))?;
        }
    }
    Ok(())
}

/// Hands off to the dashboard once setup is actually done — the reverse of
/// `bootstrap`'s `show_onboarding`. Renamed from the sibling's `open_app`
/// (which shows a single hosted instance's webview); here it just reveals
/// the multi-instance dashboard, which manages its own Decks from there.
#[tauri::command]
pub fn open_dashboard(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<'_, BootstrapState>,
) -> Result<(), BootstrapError> {
    authorize_onboarding(&window)?;
    if !state.ready.load(Ordering::Acquire) {
        return Err(BootstrapError::ActionDenied);
    }
    show_dashboard(&app)
}

/// Runs a recovery action, but only one the *last state pushed to this
/// window* actually offered (checked via `offered_actions`) — see
/// [`BootstrapError::ActionDenied`]'s doc comment for why. Deliberately a
/// small action set for now: `"retry"` re-runs `begin_setup` (the frontend
/// just calls that command directly — no server-side action needed for it,
/// it's naturally re-entrant-safe via `setup_running`), so the only action
/// actually handled here is `"quit"`. Platform-specific recovery actions
/// (reveal a log file, restart the computer) are real future work once
/// there's an actual log file and this app has been verified on Windows/
/// macOS, not fabricated here against a single Linux dev host.
#[tauri::command]
pub fn run_action(
    app: AppHandle,
    window: WebviewWindow,
    state: tauri::State<'_, BootstrapState>,
    action: String,
) -> Result<(), BootstrapError> {
    authorize_onboarding(&window)?;
    if !state
        .offered_actions
        .read()
        .map_err(|_| BootstrapError::StateLockPoisoned)?
        .contains(&action)
    {
        return Err(BootstrapError::ActionDenied);
    }
    match action.as_str() {
        "quit" => {
            app.exit(0);
            Ok(())
        }
        _ => Err(BootstrapError::ActionDenied),
    }
}

fn show_onboarding(app: &AppHandle) -> Result<(), BootstrapError> {
    let onboarding = app
        .get_webview_window("onboarding")
        .ok_or(BootstrapError::WindowMissing)?;
    onboarding
        .show()
        .map_err(|_| BootstrapError::WindowMissing)?;
    onboarding
        .set_focus()
        .map_err(|_| BootstrapError::WindowMissing)
}

fn show_dashboard(app: &AppHandle) -> Result<(), BootstrapError> {
    if let Some(onboarding) = app.get_webview_window("onboarding") {
        let _ = onboarding.hide();
    }
    let main = app
        .get_webview_window("main")
        .ok_or(BootstrapError::WindowMissing)?;
    main.show().map_err(|_| BootstrapError::WindowMissing)?;
    main.set_focus().map_err(|_| BootstrapError::WindowMissing)
}

/// Creates the isolated onboarding window, hidden by default (mirrors the
/// sibling's `hosted-app` window default) — [`bootstrap`] reveals it only if
/// setup actually turns out to be needed. Serves from `public/onboarding/`
/// (Vite copies that directory into `dist/` untouched, alongside the
/// React-bundled `index.html` at the dist root — no build-pipeline changes
/// for the dashboard). Call once from `lib.rs`'s `setup()` hook, after the
/// config-declared `"main"` window already exists.
pub fn create_onboarding_window(app: &tauri::App) -> tauri::Result<()> {
    WebviewWindowBuilder::new(
        app,
        "onboarding",
        WebviewUrl::App("onboarding/index.html".into()),
    )
    .title("Omnideck Setup")
    .inner_size(720.0, 560.0)
    .min_inner_size(640.0, 480.0)
    .resizable(false)
    .visible(false)
    .build()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_progress_is_weighted_and_monotonic() {
        assert_eq!(overall_progress(0, 0.0), 0.0);
        assert_eq!(overall_progress(0, 1.0), 0.4);
        assert_eq!(overall_progress(1, 0.0), 0.4);
        assert_eq!(overall_progress(1, 1.0), 1.0);
    }

    #[test]
    fn phase_index_matches_the_cli_stage_ids() {
        assert_eq!(phase_index("software"), Some(0));
        assert_eq!(phase_index("environment"), Some(1));
        assert_eq!(phase_index("complete"), None);
    }

    #[test]
    fn welcome_state_can_start_and_nothing_else() {
        let state = welcome_state();
        assert_eq!(state.stage, "welcome");
        assert!(state.can_start);
        assert!(!state.can_retry);
        assert!(!state.can_open);
    }

    #[test]
    fn ready_state_can_open_at_full_progress() {
        let state = ready_state();
        assert_eq!(state.stage, "ready");
        assert!(state.can_open);
        assert_eq!(state.progress, Some(1.0));
    }

    #[test]
    fn restart_required_is_not_retryable() {
        let error = CliError::Cli(Box::new(cli_bridge::CliErrorBody {
            code: "RESTART_REQUIRED".into(),
            message: "Reboot to finish enabling WSL2.".into(),
            hint: None,
            detail: None,
            action: None,
            action_value: None,
            instances: None,
        }));
        let state = error_state(&error, 0);
        assert_eq!(state.stage, "error");
        assert!(!state.can_retry);
        assert_eq!(state.primary_action.as_deref(), Some("quit"));
    }

    #[test]
    fn download_failed_is_retryable() {
        let error = CliError::Cli(Box::new(cli_bridge::CliErrorBody {
            code: "DOWNLOAD_FAILED".into(),
            message: "network blip".into(),
            hint: None,
            detail: None,
            action: None,
            action_value: None,
            instances: None,
        }));
        let state = error_state(&error, 0);
        assert!(state.can_retry);
        assert_eq!(state.primary_action.as_deref(), Some("retry"));
    }

    #[test]
    fn error_diagnostics_mark_the_failed_phase_and_leave_later_phases_waiting() {
        let error = CliError::Cli(Box::new(cli_bridge::CliErrorBody {
            code: "RUNTIME_SETUP_FAILED".into(),
            message: "x".into(),
            hint: None,
            detail: None,
            action: None,
            action_value: None,
            instances: None,
        }));
        let state = error_state(&error, 1);
        let diagnostics = state.diagnostics.unwrap();
        assert_eq!(diagnostics[0].status, "pass");
        assert_eq!(diagnostics[1].status, "issue");
    }

    #[test]
    fn version_too_old_is_not_retryable() {
        let error = CliError::VersionTooOld {
            minimum: "v0.10.0".into(),
            actual: "v0.9.0".into(),
        };
        let state = error_state(&error, 0);
        assert!(!state.can_retry);
        assert_eq!(state.primary_action.as_deref(), Some("quit"));
    }

    /// `cargo test` compiles with `debug_assertions` on, so the real
    /// (non-stubbed) `debug_forced_state()` is exercised here — this is the
    /// only place the env var name is allowed to appear outside the
    /// function itself and the README, so a rename can't silently drift.
    /// One test, not four, so the `std::env::set_var` calls can't race
    /// against another test doing the same on a parallel thread.
    #[test]
    fn debug_forced_state_covers_every_stage_and_nothing_else() {
        // SAFETY: single-threaded within this test function; no other test
        // touches this env var.
        for (value, expected_stage) in [
            ("welcome", "welcome"),
            ("preparing", "preparing"),
            ("permission", "preparing"),
            ("ready", "ready"),
            ("error", "error"),
        ] {
            unsafe { std::env::set_var("OMNIDECK_DEBUG_ONBOARDING_STAGE", value) };
            let state = debug_forced_state().expect("known stage must resolve");
            assert_eq!(state.stage, expected_stage);
        }

        unsafe { std::env::set_var("OMNIDECK_DEBUG_ONBOARDING_STAGE", "not-a-real-stage") };
        assert!(debug_forced_state().is_none());

        unsafe { std::env::remove_var("OMNIDECK_DEBUG_ONBOARDING_STAGE") };
        assert!(debug_forced_state().is_none());
    }

    #[test]
    fn permission_debug_stage_is_indeterminate_and_carries_status() {
        unsafe { std::env::set_var("OMNIDECK_DEBUG_ONBOARDING_STAGE", "permission") };
        let state = debug_forced_state().expect("permission stage must resolve");
        unsafe { std::env::remove_var("OMNIDECK_DEBUG_ONBOARDING_STAGE") };
        assert!(state.awaiting_permission);
        assert!(state.indeterminate);
        assert_eq!(state.status.as_deref(), Some("Password required"));
        assert_eq!(state.substage.as_deref(), Some("linux-permission"));
    }

    /// The 4 error codes new in CLI contract `3` — all retryable, per the
    /// CLI's own message/hint text for each (see `error_state`'s doc
    /// comment).
    #[test]
    fn the_four_new_contract_3_error_codes_are_all_retryable() {
        for code in [
            "PERMISSION_CANCELLED",
            "WINDOWS_FEATURES_FAILED",
            "PACKAGE_INDEX_FAILED",
            "INSTALLER_FAILED",
        ] {
            let error = CliError::Cli(Box::new(cli_bridge::CliErrorBody {
                code: code.to_owned(),
                message: "x".into(),
                hint: None,
                detail: None,
                action: None,
                action_value: None,
                instances: None,
            }));
            let state = error_state(&error, 0);
            assert!(state.can_retry, "{code} should be retryable");
            assert_eq!(state.primary_action.as_deref(), Some("retry"));
        }
    }
}
