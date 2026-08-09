//! Drives the *shared* (not per-instance) Podman runtime's first-run/repair
//! bootstrap, and owns the 3-command IPC surface (`bootstrap`/`begin_setup`/
//! `run_action`) the dashboard's React app uses to run it — modeled on
//! `omnideck/desktop` (sibling repo)'s setup flow, adapted per
//! `reference/desktop-hardening-migration-PLAN.md`'s Phase 5.
//!
//! Correcting this repo's earlier assumption (see AGENT.md): the CLI does
//! have an equivalent for "detect/install podman, WSL2, podman machine" as
//! of `v0.10.0` — `runtime status`/`runtime ensure` (`cmd/runtime.go` in the
//! CLI repo). This module is a thin driver over that NDJSON stream
//! ([`cli_bridge::runtime_ensure`]), translating its `stage`/`activity`/
//! error-code vocabulary into [`SetupState`] — it does not reimplement any
//! platform-specific installer logic itself.
//!
//! **No separate window** (a real change from an earlier version of this
//! module, not the original design): onboarding was originally an isolated
//! window, hidden until needed, mirroring the sibling's `hosted-app`/setup
//! window split. That was suspected (wrongly, see below) to have caused a
//! real, confirmed bug — an `EGL_BAD_PARAMETER` failure at startup on at
//! least one real Intel/Mesa combination, blank white dashboard as the
//! symptom. Fixed by dropping the second window: `bootstrap`/`begin_setup`/
//! `run_action` now run from the single `"main"` window, and
//! `src/components/OnboardingView.tsx` / `DashboardView.tsx` are just two
//! screens React swaps between based on the pushed [`SetupState`] — no
//! window to show or hide, so no `open_dashboard` command either.
//!
//! **No `Channel`, no `WebviewWindow` parameter either** (a second, later
//! correction): the single-window fix above shipped and still reproduced
//! the identical `EGL_BAD_PARAMETER` crash on the same real hardware. That
//! ruled out window *count* as the cause — the actual variable was that the
//! very first "confirmed fix" test never exercised `bootstrap` at all
//! (disabling the second window's creation also meant its JS, the only
//! thing that called `bootstrap`, never ran). Two mechanisms in this module
//! were — before this fix — unique in the whole codebase: `tauri::ipc::
//! Channel` for pushing [`SetupState`] (every other stream, `add`/`update`/
//! `remove`'s progress, uses plain `app.emit()` + frontend `listen()`,
//! proven to work in production), and a `WebviewWindow` injected command
//! parameter (every other command takes only `AppHandle`). Rather than
//! guess further, this module was converted to match the rest of the app
//! exactly: `app.emit("setup-state", ...)` instead of a `Channel`, and
//! `AppHandle`-only command signatures — dropping the manual
//! `window.label() == "main"` check along with it, since the capability
//! grant's own `"windows": ["main"]` scoping (`capabilities/default.json`)
//! already enforces the same thing at the Tauri-core level, exactly as it
//! does for every other command here; the manual check was redundant
//! defense-in-depth, never the only guard. If a future test confirms this
//! *also* wasn't the cause, the next thing to suspect is the CLI subprocess
//! spawn itself (`cli_bridge::runtime_status`) happening during initial
//! webview mount — untested in isolation as of this writing.
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

use crate::cli_bridge::{self, CliError, RuntimeSetupEvent};
use serde::Serialize;
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
};
use tauri::{AppHandle, Emitter};

/// The single event name every pushed [`SetupState`] goes out under —
/// deliberately one fixed name, not per-call-site names the way `add`/
/// `update`/`remove`'s progress events are (`"add-progress"` etc.): there's
/// only ever one onboarding screen listening, and only one bootstrap flow
/// running at a time (`BootstrapState::setup_running` already enforces
/// that), so there's nothing for a second name to disambiguate.
const SETUP_STATE_EVENT: &str = "setup-state";

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

/// Pushed from Rust to the dashboard's React app as a `"setup-state"` Tauri
/// event (`app.emit`), the same mechanism `add`/`update`/`remove`'s
/// progress already use. `OnboardingView`'s `render(state)`-style logic
/// fully re-derives what it shows from each pushed state; `App.tsx` decides
/// whether to render `OnboardingView` or `DashboardView` based on
/// `stage`/`canOpen`. Modeled on the sibling's `SetupState` in `parity.rs`.
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
/// just be a second place for the two to drift.
fn preparing_state(phase: Option<usize>, fraction: f64, activity: Option<String>) -> SetupState {
    let mut state = base_state(
        "preparing",
        "Setting things up",
        "This can take a few minutes the first time.",
    );
    state.progress = phase.map(|index| overall_progress(index, fraction));
    state.indeterminate = phase.is_none();
    state.activity = activity.or_else(|| phase.map(|index| PHASES[index].1.to_owned()));
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

/// Every failure mode of the 3 bootstrap commands themselves — distinct
/// from [`CliError`] (which is specifically about CLI subprocess failures),
/// since these also cover the action-allowlist check and event emission,
/// which have nothing to do with the CLI.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BootstrapError {
    Cli(CliError),
    /// The requested `run_action` wasn't in the *current* state's offered
    /// actions — stops a compromised/buggy webview invoking something the
    /// current state never offered.
    ActionDenied,
    StateLockPoisoned,
    StateDeliveryFailed,
}

impl From<CliError> for BootstrapError {
    fn from(error: CliError) -> Self {
        BootstrapError::Cli(error)
    }
}

#[derive(Clone)]
pub struct BootstrapState {
    setup_running: Arc<AtomicBool>,
    offered_actions: Arc<RwLock<HashSet<String>>>,
}

impl Default for BootstrapState {
    fn default() -> Self {
        Self {
            setup_running: Arc::new(AtomicBool::new(false)),
            offered_actions: Arc::new(RwLock::new(HashSet::new())),
        }
    }
}

fn send_state(
    app: &AppHandle,
    state: &BootstrapState,
    setup_state: SetupState,
) -> Result<(), BootstrapError> {
    let mut actions = state
        .offered_actions
        .write()
        .map_err(|_| BootstrapError::StateLockPoisoned)?;
    actions.clear();
    actions.extend(setup_state.primary_action.iter().cloned());
    actions.extend(setup_state.secondary_action.iter().cloned());
    drop(actions);
    app.emit(SETUP_STATE_EVENT, setup_state)
        .map_err(|_| BootstrapError::StateDeliveryFailed)
}

/// Lets a developer preview any onboarding screen on demand, without
/// touching real Podman state — set `OMNIDECK_DEBUG_ONBOARDING_STAGE` to
/// `welcome`/`preparing`/`ready`/`error` and launch `npm run dev:app`. See
/// the README's "Testing the onboarding flow" section.
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
        ),
        "ready" => ready_state(),
        "error" => error_state(
            &CliError::Cli(Box::new(cli_bridge::CliErrorBody {
                code: "DOWNLOAD_FAILED".to_owned(),
                message: format!(
                    "Simulated failure — set by OMNIDECK_DEBUG_ONBOARDING_STAGE={stage}."
                ),
                hint: None,
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
/// run — called by the dashboard's React app on mount, mirroring the
/// sibling's `setup.js` calling `bootstrap` immediately (just from the
/// dashboard's own window now, not a separate one). Never mutates anything.
/// The frontend decides what to render purely from the pushed
/// [`SetupState`] — this command doesn't show or hide anything itself.
#[tauri::command]
pub async fn bootstrap(
    app: AppHandle,
    state: tauri::State<'_, BootstrapState>,
) -> Result<(), BootstrapError> {
    if let Some(forced) = debug_forced_state() {
        send_state(&app, &state, forced)?;
        return Ok(());
    }

    match cli_bridge::runtime_status(&app).await {
        Ok(status) if status.ready => {
            send_state(&app, &state, ready_state())?;
        }
        Ok(_) => {
            send_state(&app, &state, welcome_state())?;
        }
        Err(error) => {
            send_state(&app, &state, error_state(&error, 0))?;
        }
    }
    Ok(())
}

/// Drives `runtime ensure`, streaming progress to the dashboard's React app
/// until the shared runtime is ready or setup fails. Re-entrant calls while
/// already running are ignored (matches the sibling's `setup_running` swap).
#[tauri::command]
pub async fn begin_setup(
    app: AppHandle,
    state: tauri::State<'_, BootstrapState>,
) -> Result<(), BootstrapError> {
    if state.setup_running.swap(true, Ordering::AcqRel) {
        return Ok(());
    }

    send_state(&app, &state, preparing_state(None, 0.0, None))?;

    let progress_app = app.clone();
    let progress_state = state.inner().clone();
    let result = cli_bridge::runtime_ensure(&app, move |event: RuntimeSetupEvent| {
        let index = phase_index(&event.stage);
        let fraction = event.progress.unwrap_or(0.0);
        // Prefer the CLI's `detail` when present — it's the more specific of
        // the two ("Downloading Podman for macOS…" vs. just "Getting your
        // computer ready…") — falling back to `activity`.
        let activity = event.detail.or(event.activity);
        let _ = send_state(
            &progress_app,
            &progress_state,
            preparing_state(index, fraction, activity),
        );
    })
    .await;

    state.setup_running.store(false, Ordering::Release);

    match result {
        Ok(status) if status.ready => {
            send_state(&app, &state, ready_state())?;
        }
        Ok(status) => {
            let error = CliError::Cli(Box::new(cli_bridge::CliErrorBody {
                code: "RUNTIME_SETUP_FAILED".to_owned(),
                message: format!(
                    "Podman setup finished, but the runtime is still not ready ({}).",
                    status.state
                ),
                hint: None,
                action: None,
                action_value: None,
                instances: None,
            }));
            send_state(&app, &state, error_state(&error, PHASES.len()))?;
        }
        Err(error) => {
            let reached = phase_index("environment").unwrap_or(PHASES.len() - 1);
            send_state(&app, &state, error_state(&error, reached))?;
        }
    }
    Ok(())
}

/// Runs a recovery action, but only one the *last state pushed* actually
/// offered (checked via `offered_actions`) — see
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
    state: tauri::State<'_, BootstrapState>,
    action: String,
) -> Result<(), BootstrapError> {
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
}
