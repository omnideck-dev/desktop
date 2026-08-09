
# Omnideck Desktop App — Tauri v2 POC Specification

## 1. Overview

This spec describes a proof-of-concept desktop application built with **Tauri v2** that wraps the existing Omnideck CLI + Docker container + React web app into a single installable desktop app. The goal is to eliminate the need for users to install a CLI tool, run terminal commands, or manage browser tabs — they download one app, click install, and everything happens inside that window.

**Current user flow (today):**
1. Install CLI via `brew install omnideck-dev/tap/omnideck` or download binary
2. Run `omnideck install` in terminal (interactive TUI wizard)
3. Open browser to `http://localhost:2337`
4. Complete web-based setup wizard (LLM provider, model selection)

**Desired user flow (with this app):**
1. Download and install the desktop app (.dmg / .exe / .deb)
2. Click "Install" — app detects Docker/Podman, runs install, streams output live
3. Dashboard tab auto-opens when the web app is ready
4. Everything stays inside the desktop app window

---

## 2. Goals & Non-Goals

### Goals
- Single downloadable desktop app (no terminal, no brew, no browser tabs)
- Detect Docker/Podman at startup and guide the user
- Run `omnideck install` from within the app with live stdout streaming
- Embed the existing React app at `localhost:2337` inside the desktop window
- Tab-based UI: Setup tab + Dashboard tab
- Build to `.dmg` (macOS), `.exe` (Windows), `.deb` (Linux)

### Non-Goals (for this POC)
- Full interactive TUI terminal (use `--plain` non-interactive mode instead)
- Bundling the `omnideck` CLI binary inside the app (assume it's on PATH for Phase 1)
- Bundling Docker/Podman inside the app
- Code signing / notarization
- Auto-updates
- System tray integration
- Mobile support

---

## 3. Current Architecture

```
┌─────────────────────────────────────────────────────┐
│  HOST MACHINE                                        │
│                                                      │
│  ┌──────────────┐    ┌────────────────────────────┐ │
│  │  omnideck    │    │  Container (Docker/Podman)  │ │
│  │  CLI (Go)    │───▶│  ghcr.io/omnideck-dev/...  │ │
│  │              │    │                            │ │
│  │  install     │    │  Python aiohttp (:8080)    │ │
│  │  start/stop  │    │  React 18 SPA (static)     │ │
│  │  status      │    │  WebSocket /api/browser/*  │ │
│  │  doctor      │    │  Streaming /api/chat       │ │
│  └──────────────┘    └────────────────────────────┘ │
│                      Port 2337 ──▶ 8080              │
│  ┌──────────────┐                                   │
│  │  Ollama      │◀─── (optional, localhost:11434)   │
│  └──────────────┘                                   │
└─────────────────────────────────────────────────────┘
```

### Key details
- **CLI**: Go binary, installed via Homebrew or GitHub releases. Config at `~/.config/omnideck-cli/instances/<name>.yaml`.
- **Install wizard**: Bubble Tea TUI. Detects engine (prefers Podman), checks Ollama at `localhost:11434`, suggests memory (20% host RAM, clamped 1–8 GB), suggests `shm_size`, pulls image, creates volumes, starts container.
- **Non-interactive mode**: `omnideck install --plain --engine docker --port 2337` skips the TUI entirely.
- **Web app**: React 18 + Vite SPA served by Python aiohttp inside the container. Uses WebSockets (`/api/browser/control`) and streaming fetch (`POST /api/chat` → newline-delimited JSON).
- **CLI commands**: `install`, `update`, `start`, `stop`, `restart`, `status`, `logs -f`, `doctor`, `config show`, `config set`, `uninstall`, `tui`.

---

## 4. Proposed Architecture

```
┌──────────────────────────────────────────────────────┐
│                 Tauri Desktop Window                  │
│                                                       │
│  ┌─────────────────────────────────────────────────┐│
│  │  React Shell UI (Vite, bundled in app)           ││
│  │  ┌─────────┬───────────────────────────────┐    ││
│  │  │  Setup  │  Dashboard                      │    ││
│  │  ├─────────┴───────────────────────────────┤    ││
│  │  │                                           │    ││
│  │  │  Setup Tab:                               │    ││
│  │  │   - Prereq checks (Docker/Podman)         │    ││
│  │  │   - Engine selector (Docker / Podman)      │    ││
│  │  │   - Port field (default 2337)              │    ││
│  │  │   - "Install" button                       │    ││
│  │  │   - Live stdout stream (xterm.js or <pre>) │    ││
│  │  │   - Status indicator (idle/running/done)   │    ││
│  │  │   - "Open Dashboard" button when ready     │    ││
│  │  │                                           │    ││
│  │  │  Dashboard Tab:                           │    ││
│  │  │   <iframe src="http://localhost:2337" />   │    ││
│  │  │   (disabled until webapp is ready)        │    ││
│  │  └───────────────────────────────────────┘    ││
│  └─────────────────────────────────────────────────┘│
│                      ↕ Tauri IPC                      │
│  ┌─────────────────────────────────────────────────┐│
│  │  Rust Backend (src-tauri/src/lib.rs)             ││
│  │  - check_container_engine() → std::process      ││
│  │  - run_install(engine, port) → shell plugin     ││
│  │  - get_status() → `omnideck status`             ││
│  │  - check_webapp_ready() → HTTP GET localhost     ││
│  │  - container_lifecycle(action) → docker CLI     ││
│  └─────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────┘
         ↕ shells out to
┌──────────────────────────────────────────────────────┐
│  omnideck CLI (on PATH in Phase 1, sidecar Phase 2)   │
│  → Docker/Podman → Container → :2337                  │
└──────────────────────────────────────────────────────┘
```

---

## 5. Technical Decisions

### 5.1 Load localhost:2337 via iframe in React tabs
Simplest approach for a POC. No multi-webview configuration needed — just standard React with a tab bar and an `<iframe>`. If iframe causes issues (CSP, WebSocket), fall back to `WebviewWindow` (separate OS window) or Tauri v2 multi-webview in one window.

### 5.2 Use `omnideck install --plain` with a React form
The interactive TUI requires a full PTY bridge (Rust `portable-pty` + xterm.js with bidirectional input). For the POC, use `--plain` non-interactive mode and collect install options (engine, port) via a React form. This gives a nicer GUI experience anyway and avoids ~200 lines of PTY plumbing.

### 5.3 Assume `omnideck` is on PATH (Phase 1)
For the POC, assume the user has already installed the CLI via `brew install omnideck-dev/tap/omnideck`. Phase 2 will bundle the binary as a Tauri sidecar (`externalBin` in `tauri.conf.json`) for the true "just download the app" experience.

### 5.4 Stream stdout via Tauri events
Use `tauri-plugin-shell`'s `CommandEvent` to stream stdout/stderr lines from the `omnideck install` process to the frontend via `app.emit()`. Display in xterm.js (write-only) or a styled `<pre>` element.

### 5.5 Poll localhost:2337 for readiness
After install completes, poll `http://localhost:2337` with an HTTP GET every 2 seconds. When it responds, auto-enable the Dashboard tab and optionally auto-switch to it.

---

## 6. File Structure

```
omnideck-desktop/
├── package.json
├── vite.config.ts
├── index.html
├── tsconfig.json
├── src/                          # React frontend (the shell UI)
│   ├── main.tsx                  # React entry point
│   ├── App.tsx                   # Root component with tab state
│   ├── components/
│   │   ├── TabBar.tsx            # Tab navigation bar
│   │   ├── SetupView.tsx         # Setup tab: prereq checks + install form + stdout
│   │   ├── DashboardView.tsx     # Dashboard tab: iframe to localhost:2337
│   │   ├── StdoutDisplay.tsx     # Live stdout/stderr display (xterm.js or <pre>)
│   │   └── StatusBadge.tsx       # Status indicator component
│   ├── hooks/
│   │   ├── useInstall.ts         # Hook: invoke run_install, listen for events
│   │   ├── useContainerEngine.ts # Hook: check_container_engine on mount
│   │   └── useWebappReady.ts     # Hook: poll localhost:2337 for readiness
│   └── styles/
│       └── app.css               # Shell UI styling
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json           # Tauri config (CSP, bundle, windows)
│   ├── capabilities/
│   │   └── default.json          # Permissions for shell plugin
│   ├── icons/                    # App icons
│   └── src/
│       ├── main.rs               # Tauri entry (don't modify unless needed)
│       └── lib.rs                # All Rust commands + plugin setup
```

---

## 7. Implementation Guide

### Step 1: Scaffold the project

```bash
npm create tauri-app@latest -- --template react-ts
# Name: omnideck-desktop
# Package manager: npm
# UI template: React + TypeScript
```

### Step 2: Add the shell plugin

```bash
cd omnideck-desktop
npm run tauri add shell
```

This adds `tauri-plugin-shell` to `Cargo.toml` and registers it. Also install the JS bindings:

```bash
npm install @tauri-apps/plugin-shell
```

### Step 3: Configure permissions

`src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "shell:allow-execute",
    "shell:allow-spawn",
    "shell:allow-stdin-write",
    "shell:allow-kill"
  ]
}
```

### Step 4: Write the Rust backend

`src-tauri/src/lib.rs`:

```rust
use tauri::Manager;
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandEvent;
use std::process::Command;

/// Check if Docker or Podman is installed. Returns "docker", "podman", or "none".
#[tauri::command]
fn check_container_engine() -> String {
    if Command::new("podman").arg("--version").output().is_ok() {
        return "podman".to_string();
    }
    if Command::new("docker").arg("--version").output().is_ok() {
        return "docker".to_string();
    }
    "none".to_string()
}

/// Check if the omnideck CLI is on PATH.
#[tauri::command]
fn check_omnideck_cli() -> bool {
    Command::new("omnideck").arg("--version").output().is_ok()
}

/// Run `omnideck install --plain --engine <engine> --port <port>` non-interactively.
/// Streams stdout/stderr to the frontend via Tauri events.
#[tauri::command]
async fn run_install(app: tauri::AppHandle, engine: String, port: String) -> Result<i32, String> {
    let (mut rx, _child) = app
        .shell()
        .command("omnideck")
        .args(["install", "--plain", "--engine", &engine, "--port", &port])
        .spawn()
        .map_err(|e| e.to_string())?;

    let mut exit_code = -1;

    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(line) => {
                let text = String::from_utf8_lossy(&line).to_string();
                let _ = app.emit("install-stdout", text);
            }
            CommandEvent::Stderr(line) => {
                let text = String::from_utf8_lossy(&line).to_string();
                let _ = app.emit("install-stderr", text);
            }
            CommandEvent::Terminated(status) => {
                exit_code = status.code.unwrap_or(-1);
                let _ = app.emit("install-done", exit_code);
            }
            _ => {}
        }
    }

    Ok(exit_code)
}

/// Run `omnideck status` and return the output.
#[tauri::command]
async fn get_status() -> Result<String, String> {
    let output = Command::new("omnideck")
        .arg("status")
        .output()
        .map_err(|e| e.to_string())?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Check if the web app at localhost:PORT is responding.
#[tauri::command]
async fn check_webapp_ready(port: String) -> bool {
    let url = format!("http://localhost:{}", port);
    // Use a simple TCP connect check (no HTTP client dependency needed)
    let addr = format!("127.0.0.1:{}", port);
    std::net::TcpStream::connect(&addr).is_ok()
}

/// Container lifecycle: start, stop, restart.
#[tauri::command]
async fn container_lifecycle(app: tauri::AppHandle, action: String) -> Result<String, String> {
    let (mut rx, _child) = app
        .shell()
        .command("omnideck")
        .args([&action])
        .spawn()
        .map_err(|e| e.to_string())?;

    let mut output = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(line) => {
                output.push_str(&String::from_utf8_lossy(&line));
            }
            CommandEvent::Stderr(line) => {
                output.push_str(&String::from_utf8_lossy(&line));
            }
            CommandEvent::Terminated(_) => break,
            _ => {}
        }
    }

    Ok(output)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            check_container_engine,
            check_omnideck_cli,
            run_install,
            get_status,
            check_webapp_ready,
            container_lifecycle,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### Step 5: Configure Tauri

`src-tauri/tauri.conf.json` (key sections — merge with generated config):

```jsonc
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Omnideck",
  "version": "0.1.0",
  "identifier": "dev.omnideck.desktop",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "Omnideck",
        "width": 1200,
        "height": 800,
        "minWidth": 900,
        "minHeight": 600,
        "url": "index.html"
      }
    ],
    "security": {
      "csp": "default-src 'self'; frame-src http://localhost:2337 http://localhost:*; connect-src http://localhost:2337 http://localhost:* ws://localhost:2337 ws://localhost:*; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'; script-src 'self'"
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "shortDescription": "Omnideck Desktop",
    "longDescription": "Desktop wrapper for the Omnideck local-first AI workbench"
  }
}
```

### Step 6: Build the React shell

`src/App.tsx`:

```tsx
import { useState, useEffect } from 'react';
import TabBar from './components/TabBar';
import SetupView from './components/SetupView';
import DashboardView from './components/DashboardView';
import { useWebappReady } from './hooks/useWebappReady';
import './styles/app.css';

type Tab = 'setup' | 'dashboard';

export default function App() {
  const [tab, setTab] = useState<Tab>('setup');
  const { ready, checking } = useWebappReady('2337');

  return (
    <div className="app">
      <TabBar
        activeTab={tab}
        onTabChange={setTab}
        dashboardEnabled={ready}
      />
      <main className="content">
        {tab === 'setup' && (
          <SetupView
            webappReady={ready}
            onSwitchToDashboard={() => setTab('dashboard')}
          />
        )}
        {tab === 'dashboard' && <DashboardView />}
      </main>
    </div>
  );
}
```

`src/components/TabBar.tsx`:

```tsx
interface Props {
  activeTab: 'setup' | 'dashboard';
  onTabChange: (tab: 'setup' | 'dashboard') => void;
  dashboardEnabled: boolean;
}

export default function TabBar({ activeTab, onTabChange, dashboardEnabled }: Props) {
  return (
    <nav className="tab-bar">
      <button
        className={activeTab === 'setup' ? 'tab active' : 'tab'}
        onClick={() => onTabChange('setup')}
      >
        ⚙️ Setup
      </button>
      <button
        className={activeTab === 'dashboard' ? 'tab active' : 'tab'}
        onClick={() => onTabChange('dashboard')}
        disabled={!dashboardEnabled}
        title={dashboardEnabled ? '' : 'Omnideck must be running first'}
      >
        🖥️ Dashboard
      </button>
    </nav>
  );
}
```

`src/components/SetupView.tsx`:

```tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import StdoutDisplay from './StdoutDisplay';
import StatusBadge from './StatusBadge';

type InstallStatus = 'idle' | 'checking' | 'running' | 'done' | 'error';

export default function SetupView({
  webappReady,
  onSwitchToDashboard,
}: {
  webappReady: boolean;
  onSwitchToDashboard: () => void;
}) {
  const [engine, setEngine] = useState<string>('docker');
  const [port] = useState<string>('2337');
  const [status, setStatus] = useState<InstallStatus>('idle');
  const [output, setOutput] = useState<string[]>([]);
  const [engineDetected, setEngineDetected] = useState<string>('');
  const [cliInstalled, setCliInstalled] = useState<boolean>(false);

  useEffect(() => {
    async function checkPrereqs() {
      setStatus('checking');
      const eng = await invoke<string>('check_container_engine');
      const cli = await invoke<boolean>('check_omnideck_cli');
      setEngineDetected(eng);
      setCliInstalled(cli);
      if (eng !== 'none') setEngine(eng);
      setStatus('idle');
    }
    checkPrereqs();
  }, []);

  useEffect(() => {
    const unlistenStdout = listen<string>('install-stdout', (e) => {
      setOutput((prev) => [...prev, e.payload]);
    });
    const unlistenStderr = listen<string>('install-stderr', (e) => {
      setOutput((prev) => [...prev, `[stderr] ${e.payload}`]);
    });
    const unlistenDone = listen<number>('install-done', (e) => {
      setStatus(e.payload === 0 ? 'done' : 'error');
    });
    return () => {
      unlistenStdout.then((f) => f());
      unlistenStderr.then((f) => f());
      unlistenDone.then((f) => f());
    };
  }, []);

  async function handleInstall() {
    setOutput([]);
    setStatus('running');
    try {
      await invoke('run_install', { engine, port });
    } catch (e) {
      setStatus('error');
      setOutput((prev) => [...prev, `Error: ${e}`]);
    }
  }

  return (
    <div className="setup-view">
      <h2>Setup</h2>

      <section className="prereqs">
        <h3>Prerequisites</h3>
        <div className="check-row">
          <StatusBadge
            ok={engineDetected !== 'none'}
            label={`Container Engine: ${engineDetected === 'none' ? 'Not found' : engineDetected}`}
          />
          <StatusBadge
            ok={cliInstalled}
            label={`Omnideck CLI: ${cliInstalled ? 'Installed' : 'Not found (run: brew install omnideck-dev/tap/omnideck)'}`}
          />
        </div>
      </section>

      <section className="install-form">
        <h3>Installation</h3>
        <label>
          Container Engine:
          <select value={engine} onChange={(e) => setEngine(e.target.value)}>
            <option value="docker">Docker</option>
            <option value="podman">Podman</option>
          </select>
        </label>
        <button
          onClick={handleInstall}
          disabled={status === 'running' || status === 'checking' || engineDetected === 'none' || !cliInstalled}
        >
          {status === 'running' ? 'Installing...' : 'Install Omnideck'}
        </button>
      </section>

      {output.length > 0 && (
        <section className="output">
          <h3>Install Log</h3>
          <StdoutDisplay lines={output} />
        </section>
      )}

      {status === 'done' && (
        <section className="post-install">
          <p>✅ Installation complete!</p>
          {webappReady ? (
            <button onClick={onSwitchToDashboard}>Open Dashboard →</button>
          ) : (
            <p>Waiting for web app to start...</p>
          )}
        </section>
      )}

      {status === 'error' && (
        <section className="post-install">
          <p>❌ Installation failed. Check the log above.</p>
        </section>
      )}
    </div>
  );
}
```

`src/components/DashboardView.tsx`:

```tsx
export default function DashboardView() {
  return (
    <div className="dashboard-view">
      <iframe
        src="http://localhost:2337"
        className="webapp-frame"
        allow="clipboard-read; clipboard-write; fullscreen"
      />
    </div>
  );
}
```

`src/components/StdoutDisplay.tsx`:

```tsx
import { useEffect, useRef } from 'react';

export default function StdoutDisplay({ lines }: { lines: string[] }) {
  const ref = useRef<HTMLPreElement>(null);

  useEffect(() => {
    if (ref.current) {
      ref.current.scrollTop = ref.current.scrollHeight;
    }
  }, [lines]);

  return (
    <pre ref={ref} className="stdout-display">
      {lines.join('')}
    </pre>
  );
}
```

`src/components/StatusBadge.tsx`:

```tsx
export default function StatusBadge({ ok, label }: { ok: boolean; label: string }) {
  return (
    <span className={`badge ${ok ? 'badge-ok' : 'badge-error'}`}>
      {ok ? '✅' : '❌'} {label}
    </span>
  );
}
```

`src/hooks/useWebappReady.ts`:

```ts
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

export function useWebappReady(port: string) {
  const [ready, setReady] = useState(false);
  const [checking, setChecking] = useState(true);

  useEffect(() => {
    const interval = setInterval(async () => {
      const isReady = await invoke<boolean>('check_webapp_ready', { port });
      if (isReady) {
        setReady(true);
        setChecking(false);
        clearInterval(interval);
      }
    }, 2000);

    return () => clearInterval(interval);
  }, [port]);

  return { ready, checking };
}
```

`src/styles/app.css`:

```css
* { margin: 0; padding: 0; box-sizing: border-box; }

.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  background: #1a1a2e;
  color: #e0e0e0;
}

.tab-bar {
  display: flex;
  gap: 0;
  background: #16213e;
  border-bottom: 1px solid #0f3460;
  padding: 0 1rem;
}

.tab {
  padding: 0.75rem 1.5rem;
  background: transparent;
  border: none;
  color: #8892b0;
  cursor: pointer;
  font-size: 0.95rem;
  border-bottom: 2px solid transparent;
  transition: all 0.2s;
}

.tab:hover:not(:disabled) { color: #e0e0e0; }
.tab.active { color: #64ffda; border-bottom-color: #64ffda; }
.tab:disabled { opacity: 0.4; cursor: not-allowed; }

.content { flex: 1; overflow: auto; }

/* Setup view */
.setup-view { padding: 2rem; max-width: 700px; }
.setup-view h2 { margin-bottom: 1.5rem; color: #64ffda; }
.setup-view h3 { margin: 1rem 0 0.5rem; color: #ccd6f6; }

.prereqs, .install-form, .output, .post-install {
  margin-bottom: 1.5rem;
  padding: 1rem;
  background: #16213e;
  border-radius: 8px;
}

.check-row { display: flex; flex-direction: column; gap: 0.5rem; }

.badge {
  display: inline-block;
  padding: 0.4rem 0.8rem;
  border-radius: 4px;
  font-size: 0.85rem;
}
.badge-ok { background: rgba(100, 255, 218, 0.1); color: #64ffda; }
.badge-error { background: rgba(255, 100, 100, 0.1); color: #ff6b6b; }

.install-form label { display: block; margin-bottom: 1rem; }
.install-form select {
  margin-left: 0.5rem;
  padding: 0.3rem;
  background: #0f3460;
  color: #e0e0e0;
  border: 1px solid #303956;
  border-radius: 4px;
}

.install-form button {
  padding: 0.6rem 1.5rem;
  background: #0f3460;
  color: #64ffda;
  border: 1px solid #64ffda;
  border-radius: 6px;
  cursor: pointer;
  font-size: 0.95rem;
}
.install-form button:hover:not(:disabled) { background: #1a4a7a; }
.install-form button:disabled { opacity: 0.5; cursor: not-allowed; }

.stdout-display {
  background: #0d1117;
  color: #c9d1d9;
  padding: 1rem;
  border-radius: 6px;
  font-family: 'JetBrains Mono', 'Fira Code', monospace;
  font-size: 0.85rem;
  max-height: 300px;
  overflow-y: auto;
  white-space: pre-wrap;
}

.post-install button {
  padding: 0.6rem 1.5rem;
  background: #64ffda;
  color: #1a1a2e;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-weight: 600;
}

/* Dashboard view */
.dashboard-view { height: 100%; }
.webapp-frame {
  width: 100%;
  height: 100%;
  border: none;
}
```

### Step 8: Test

```bash
# Dev mode (hot reload for frontend + Rust)
npm run tauri dev

# Test the full flow:
# 1. App opens on Setup tab
# 2. Prereq checks run (Docker/Podman + omnideck CLI)
# 3. Click "Install Omnideck"
# 4. Watch stdout stream in the log area
# 5. When done, Dashboard tab auto-enables
# 6. Click "Open Dashboard" → iframe loads localhost:2337
```

### Step 9: Build

```bash
# Build for current platform (produces .dmg on macOS, .exe on Windows, .deb on Linux)
npm run tauri build

# Or target a specific bundle format:
npm run tauri build -- --bundles dmg    # macOS only
npm run tauri build -- --bundles nsis    # Windows only
npm run tauri build -- --bundles deb     # Linux only
```

---

## 8. CSP Configuration

The Content Security Policy must allow the iframe to load `localhost:2337` and permit WebSocket connections (used by the React app for browser control):

```jsonc
"security": {
  "csp": "default-src 'self'; frame-src http://localhost:2337 http://localhost:*; connect-src http://localhost:2337 http://localhost:* ws://localhost:2337 ws://localhost:*; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'; script-src 'self'"
}
```

**Why each directive:**
- `frame-src http://localhost:2337 http://localhost:*` — allows the iframe to load the Omnideck web app (wildcard for custom ports in multi-instance setups)
- `connect-src http://localhost:* ws://localhost:*` — allows fetch + WebSocket from the React app (streaming chat, browser control)
- `img-src 'self' data: https:` — allows inline images and screenshots from the web app
- `style-src 'self' 'unsafe-inline'` — allows the web app's CSS (many React libs use inline styles)
- `script-src 'self'` — only allow scripts from the app bundle (no external CDN scripts)

---

## 9. Phased Roadmap

### Phase 1 — POC (1–2 days)
- [x] Scaffold Tauri v2 + React + TypeScript project
- [x] Add `tauri-plugin-shell` and configure permissions
- [x] Write Rust commands: `check_container_engine`, `check_omnideck_cli`, `run_install`, `get_status`, `check_webapp_ready`, `container_lifecycle`
- [x] Build React shell: TabBar + SetupView + DashboardView
- [x] Stream install stdout via Tauri events
- [x] Poll localhost:2337 for webapp readiness
- [x] iframe to localhost:2337 in Dashboard tab
- [x] Build to `.dmg` / `.exe` / `.deb`

### Phase 2 — Polish (post-POC, if concept is validated)
- [ ] Bundle `omnideck` CLI binary as a Tauri sidecar (`externalBin` in config)
- [ ] Full PTY terminal with `portable-pty` + xterm.js for interactive TUI mode
- [ ] Auto-switch to Dashboard tab when webapp becomes ready
- [ ] Container lifecycle controls (start/stop/restart buttons in the UI)
- [ ] `omnideck doctor` health check integration
- [ ] `omnideck update` button (pull latest image, recreate container)
- [ ] System tray icon with quick actions
- [ ] Auto-updates via Tauri updater plugin
- [ ] Code signing + notarization (macOS) / code signing (Windows)
- [ ] Multi-instance support (manage multiple Omnideck instances from one app)
- [ ] Ollama detection and status in the Setup tab

### Phase 3 — Production (future)
- [ ] Bundle Podman as a sidecar (eliminate Docker dependency entirely)
- [ ] First-run wizard with animated progress
- [ ] Deep links / URL scheme (`omnideck://`)
- [ ] Notification center integration
- [ ] Mobile companion app (Tauri v2 supports iOS/Android)

---

## 10. Risks & Mitigations

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|-----------|
| iframe blocked by CSP | High | Medium | Explicitly allow `frame-src` and `connect-src` for localhost. Fall back to `WebviewWindow` if iframe fails. |
| WebSocket not working through iframe | Medium | Low | CSP includes `ws://localhost:*` in `connect-src`. Test browser control feature specifically. |
| `omnideck` CLI not on PATH | High | Medium (Phase 1) | Show clear error message with install instructions. Phase 2 bundles as sidecar. |
| Docker/Podman not installed | High | Medium | Detect at startup, show friendly message with install links. |
| `--plain` mode missing options | Medium | Low | Check `omnideck install --help` for available flags. May need to add `--memory`, `--shm-size` flags if not present. |
| macOS quarantine on unsigned app | Low | High | Document `xattr -rd com.apple.quarantine /path/to/app` workaround. Phase 2 adds signing. |
| stdout streaming misses TUI formatting | Low | Expected | Acceptable for POC. The `--plain` mode outputs plain text, not TUI escape sequences. |
| First Rust build is slow (3–5 min) | Low | Certain | Document this. Subsequent builds use incremental compilation and are fast. |

---

## 11. Testing Checklist

### Prerequisites
- [ ] Docker or Podman installed and running
- [ ] `omnideck` CLI installed (`brew install omnideck-dev/tap/omnideck`)
- [ ] Rust toolchain installed (for building: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- [ ] Node.js 18+ installed

### Dev mode
- [ ] `npm run tauri dev` starts without errors
- [ ] App window opens on Setup tab
- [ ] Prereq checks show correct Docker/Podman status
- [ ] Prereq checks show correct omnideck CLI status
- [ ] Engine selector defaults to detected engine
- [ ] Install button is disabled if prereqs missing
- [ ] Clicking Install streams stdout live to the log area
- [ ] Install completes with exit code 0 → status shows "done"
- [ ] Dashboard tab becomes enabled after webapp is ready
- [ ] Clicking "Open Dashboard" shows the Omnideck React app in the iframe
- [ ] Chat functionality works inside the iframe (test sending a message)
- [ ] Browser control / preview works inside the iframe (test WebSocket)

### Build
- [ ] `npm run tauri build` completes without errors
- [ ] Produced `.dmg` (or `.exe`/`.deb`) installs and runs
- [ ] App works the same as in dev mode

### Error cases
- [ ] App shows clear error if Docker/Podman not found
- [ ] App shows clear error if `omnideck` CLI not found
- [ ] App shows error if install fails (non-zero exit code)
- [ ] Dashboard tab stays disabled if webapp never comes up

---

## 12. References

- **Tauri v2 docs**: https://v2.tauri.app/
- **Tauri shell plugin**: https://v2.tauri.app/plugin/shell/
- **Tauri sidecar**: https://v2.tauri.app/develop/sidecar/
- **Tauri CSP config**: https://v2.tauri.app/security/csp/
- **Tauri bundling**: https://v2.tauri.app/distribute/
- **Tauri multi-webview**: https://v2.tauri.app/reference/config/
- **xterm.js + Tauri example (Kerminal)**: https://github.com/klpod221/kerminal
- **portable-pty crate**: https://docs.rs/portable-pty/latest/portable_pty/
- **Omnideck docs**: https://omnideck.dev/docs/
- **Omnideck CLI repo**: https://github.com/omnideck-dev/cli
- **Omnideck main repo**: https://github.com/omnideck-dev/omnideck

---
