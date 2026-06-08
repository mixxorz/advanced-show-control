# Phase 6 Tauri Thin Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a durable Tauri desktop shell with a stateless React frontend that renders Rust-owned LV1 app snapshots.

**Architecture:** The existing Rust crate remains the protocol and fade core. A new `src-tauri` crate owns desktop runtime state, Tauri commands, event forwarding, and serializable snapshots. A new `ui` React + TypeScript + Vite app renders snapshots, keeps only active-tab and form-input state locally, and sends user intent through Tauri commands.

**Tech Stack:** Rust 2024, Tauri 2, tokio, serde, React, TypeScript, Vite, Tailwind CSS 4, npm.

---

## File Map

| File | Status | Responsibility |
|---|---|---|
| `.gitignore` | Modify | Ignore visual companion scratch files and frontend build artifacts |
| `Cargo.toml` | Modify | Add workspace members once `src-tauri` exists |
| `package.json` | Create | Root npm scripts for frontend and Tauri commands |
| `ui/package.json` | Create | Frontend package dependencies and scripts |
| `ui/index.html` | Create | Vite HTML entry |
| `ui/tsconfig.json` | Create | TypeScript configuration |
| `ui/vite.config.ts` | Create | Vite React configuration |
| `ui/src/main.tsx` | Create | React app entry |
| `ui/src/App.tsx` | Create | Stateless shell renderer and command dispatch |
| `ui/src/types.ts` | Create | Frontend DTOs matching Rust snapshots |
| `ui/src/index.css` | Create | Tailwind 4 import and global base styling |
| `src-tauri/Cargo.toml` | Create | Tauri desktop host crate manifest |
| `src-tauri/tauri.conf.json` | Create | Tauri app configuration |
| `src-tauri/capabilities/default.json` | Create | Tauri frontend permissions |
| `src-tauri/build.rs` | Create | Tauri build hook |
| `src-tauri/src/main.rs` | Create | Tauri entrypoint |
| `src-tauri/src/app_state.rs` | Create | Rust-owned app state, snapshot DTOs, logs, tests |
| `src-tauri/src/commands.rs` | Create | Tauri command handlers |

---

## Task 1: Prepare Workspace And Ignore Scratch Files

**Files:**
- Modify: `.gitignore`

- [ ] **Step 1: Add ignored generated directories**

Update `.gitignore` to exactly:

```gitignore
/target
logs/
.superpowers/
node_modules/
ui/node_modules/
ui/dist/
src-tauri/target/
```

- [ ] **Step 2: Verify the current Rust tests still pass**

Run: `cargo test`

Expected: all existing tests pass.

- [ ] **Step 3: Commit**

```bash
git add .gitignore
git commit -m "chore: prepare workspace for tauri shell"
```

---

## Task 2: Scaffold Frontend Package

**Files:**
- Create: `package.json`
- Create: `ui/package.json`
- Create: `ui/index.html`
- Create: `ui/tsconfig.json`
- Create: `ui/vite.config.ts`
- Create: `ui/src/main.tsx`
- Create: `ui/src/App.tsx`
- Create: `ui/src/types.ts`
- Create: `ui/src/index.css`

- [ ] **Step 1: Create root npm scripts**

Create `package.json`:

```json
{
  "scripts": {
    "dev": "npm --prefix ui run dev",
    "build": "npm --prefix ui run build",
    "typecheck": "npm --prefix ui run typecheck",
    "tauri": "tauri"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.0.0"
  }
}
```

- [ ] **Step 2: Create frontend package manifest**

Create `ui/package.json`:

```json
{
  "name": "lv1-scene-fade-ui",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite --host 127.0.0.1",
    "build": "vite build",
    "typecheck": "tsc --noEmit"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.0.0",
    "@vitejs/plugin-react": "^5.0.0",
    "tailwindcss": "^4.0.0",
    "@tailwindcss/vite": "^4.0.0",
    "vite": "^7.0.0",
    "typescript": "^5.0.0",
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0"
  }
}
```

- [ ] **Step 3: Create Vite entry files**

Create `ui/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>LV1 Scene Fade Utility</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

Create `ui/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "lib": ["DOM", "DOM.Iterable", "ES2022"],
    "allowJs": false,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "strict": true,
    "forceConsistentCasingInFileNames": true,
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx"
  },
  "include": ["src"],
  "references": []
}
```

Create `ui/vite.config.ts`:

```ts
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
});
```

- [ ] **Step 4: Create initial frontend types**

Create `ui/src/types.ts`:

```ts
export type ConnectionState = "disconnected" | "connecting" | "connected";
export type FadeState = "idle" | "running" | "blocked";
export type LogSource = "app" | "lv1" | "fade";
export type LogSeverity = "info" | "warning" | "error";

export type SceneSummary = {
  index: number;
  name: string;
};

export type AppLogEntry = {
  id: number;
  timestamp: string;
  source: LogSource;
  severity: LogSeverity;
  message: string;
};

export type AppSnapshot = {
  connection: ConnectionState;
  currentScene: SceneSummary | null;
  scenes: SceneSummary[];
  sceneCount: number;
  channelCount: number;
  fadeState: FadeState;
  lockout: boolean;
  logs: AppLogEntry[];
  lastEventAt: string | null;
};
```

- [ ] **Step 5: Create a static React shell**

Create `ui/src/main.tsx`:

```tsx
import React from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

Create `ui/src/App.tsx`:

```tsx
import { useState } from "react";
import type { AppSnapshot } from "./types";

const initialSnapshot: AppSnapshot = {
  connection: "disconnected",
  currentScene: null,
  scenes: [],
  sceneCount: 0,
  channelCount: 0,
  fadeState: "idle",
  lockout: false,
  logs: [],
  lastEventAt: null,
};

type Tab = "connection" | "scene" | "logs";

export default function App() {
  const [activeTab, setActiveTab] = useState<Tab>("connection");
  const snapshot = initialSnapshot;

  return (
    <main className="min-h-screen bg-slate-950 text-slate-100">
      <header className="border-b border-slate-800 bg-slate-900/80 px-6 py-4">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold">LV1 Scene Fade Utility</h1>
            <p className="text-sm text-slate-400">Desktop shell</p>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <StatusBadge label={snapshot.connection} tone="neutral" />
            <StatusBadge label={`Fade: ${snapshot.fadeState}`} tone="neutral" />
            <StatusBadge label={snapshot.lockout ? "Lockout On" : "Lockout Off"} tone={snapshot.lockout ? "warning" : "neutral"} />
            <button className="rounded-lg bg-red-700 px-5 py-3 font-bold text-white shadow-lg shadow-red-950/40 hover:bg-red-600">
              Abort All
            </button>
          </div>
        </div>
      </header>

      <nav className="border-b border-slate-800 px-6">
        <div className="flex gap-2">
          <TabButton active={activeTab === "connection"} onClick={() => setActiveTab("connection")}>Connection</TabButton>
          <TabButton active={activeTab === "scene"} onClick={() => setActiveTab("scene")}>Scene</TabButton>
          <TabButton active={activeTab === "logs"} onClick={() => setActiveTab("logs")}>Logs</TabButton>
        </div>
      </nav>

      <section className="p-6">
        {activeTab === "connection" && <ConnectionTab snapshot={snapshot} />}
        {activeTab === "scene" && <SceneTab snapshot={snapshot} />}
        {activeTab === "logs" && <LogsTab snapshot={snapshot} />}
      </section>
    </main>
  );
}

function TabButton(props: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      className={props.active ? "border-b-2 border-cyan-400 px-4 py-3 text-cyan-200" : "px-4 py-3 text-slate-400 hover:text-slate-100"}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}

function StatusBadge(props: { label: string; tone: "neutral" | "warning" }) {
  const tone = props.tone === "warning" ? "border-amber-500/60 bg-amber-950 text-amber-100" : "border-slate-700 bg-slate-800 text-slate-200";
  return <span className={`rounded-full border px-3 py-1 text-sm ${tone}`}>{props.label}</span>;
}

function ConnectionTab({ snapshot }: { snapshot: AppSnapshot }) {
  return (
    <div className="rounded-xl border border-slate-800 bg-slate-900 p-5">
      <h2 className="text-lg font-semibold">Connection</h2>
      <p className="mt-2 text-slate-400">Status: {snapshot.connection}</p>
    </div>
  );
}

function SceneTab({ snapshot }: { snapshot: AppSnapshot }) {
  return (
    <div className="rounded-xl border border-slate-800 bg-slate-900 p-5">
      <h2 className="text-lg font-semibold">Scene</h2>
      <p className="mt-2 text-slate-400">Current scene: {snapshot.currentScene ? `${snapshot.currentScene.index}: ${snapshot.currentScene.name}` : "None"}</p>
      <p className="mt-1 text-slate-400">Known channels: {snapshot.channelCount}</p>
    </div>
  );
}

function LogsTab({ snapshot }: { snapshot: AppSnapshot }) {
  return (
    <div className="rounded-xl border border-slate-800 bg-slate-900 p-5">
      <h2 className="text-lg font-semibold">Logs</h2>
      <p className="mt-2 text-slate-400">{snapshot.logs.length === 0 ? "No events yet." : `${snapshot.logs.length} events`}</p>
    </div>
  );
}
```

Create `ui/src/index.css`:

```css
@import "tailwindcss";

:root {
  color-scheme: dark;
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

body {
  margin: 0;
}

button {
  font: inherit;
}
```

- [ ] **Step 6: Install frontend dependencies**

Run: `npm install`

Expected: `package-lock.json` is created and dependencies install successfully.

- [ ] **Step 7: Verify frontend type-check and build**

Run: `npm run typecheck`

Expected: PASS.

Run: `npm run build`

Expected: PASS and `ui/dist/` is generated.

- [ ] **Step 8: Commit**

```bash
git add package.json package-lock.json ui/package.json ui/package-lock.json ui/index.html ui/tsconfig.json ui/vite.config.ts ui/src/main.tsx ui/src/App.tsx ui/src/types.ts ui/src/index.css
git commit -m "feat: scaffold react tauri shell ui"
```

---

## Task 3: Add Tauri Host And Rust-Owned Snapshot Types

**Files:**
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/capabilities/default.json`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/app_state.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Create Tauri crate manifest**

In root `Cargo.toml`, add this workspace section after the `[package]` block:

```toml
[workspace]
members = [".", "src-tauri"]
resolver = "2"
```

Keep the existing `[package]`, `[lib]`, and `[dependencies]` sections unchanged.

Then create `src-tauri/Cargo.toml`:


```toml
[package]
name = "lv1-scene-fade-utility-tauri"
version = "0.1.0"
description = "Desktop shell for LV1 Scene Fade Utility"
edition = "2024"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
lv1-scene-fade-utility = { path = ".." }
serde = { version = "1.0", features = ["derive"] }
tauri = { version = "2", features = [] }
tokio = { version = "1", features = ["sync", "time", "rt-multi-thread", "macros"] }
```

- [ ] **Step 2: Create Tauri config and build hook**

Create `src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "LV1 Scene Fade Utility",
  "version": "0.1.0",
  "identifier": "com.lv1scenefade.utility",
  "build": {
    "beforeDevCommand": "npm --prefix ../ui run dev",
    "beforeBuildCommand": "npm --prefix ../ui run build",
    "devUrl": "http://127.0.0.1:1420",
    "frontendDist": "../ui/dist"
  },
  "app": {
    "windows": [
      {
        "title": "LV1 Scene Fade Utility",
        "width": 1180,
        "height": 780,
        "minWidth": 960,
        "minHeight": 640
      }
    ]
  },
  "bundle": {
    "active": false,
    "targets": "all"
  }
}
```

Create `src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default app capability",
  "windows": ["main"],
  "permissions": ["core:default", "core:event:default"]
}
```

Create `src-tauri/build.rs`:

```rust
fn main() {
    tauri_build::build();
}
```

- [ ] **Step 3: Write Rust snapshot tests first**

Create `src-tauri/src/app_state.rs` with tests and minimal type definitions:

```rust
use std::collections::VecDeque;
use std::sync::Arc;

use lv1_scene_fade_utility::lv1::state::{ConnectionStatus, Lv1StateSnapshot};
use serde::Serialize;
use tokio::sync::Mutex;

const MAX_LOGS: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SceneSummary {
    pub index: i32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppLogEntry {
    pub id: u64,
    pub timestamp: String,
    pub source: LogSource,
    pub severity: LogSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LogSource {
    App,
    Lv1,
    Fade,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LogSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AppConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AppFadeState {
    Idle,
    Running,
    Blocked,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub connection: AppConnectionState,
    pub current_scene: Option<SceneSummary>,
    pub scenes: Vec<SceneSummary>,
    pub scene_count: usize,
    pub channel_count: usize,
    pub fade_state: AppFadeState,
    pub lockout: bool,
    pub logs: Vec<AppLogEntry>,
    pub last_event_at: Option<String>,
}

#[derive(Default)]
pub struct RuntimeHandles {
    pub lv1: Option<lv1_scene_fade_utility::lv1::handle::Lv1ActorHandle>,
    pub fade: Option<lv1_scene_fade_utility::fade::engine::FadeEngineHandle>,
}

#[derive(Clone)]
pub struct ShellState {
    pub handles: Arc<Mutex<RuntimeHandles>>,
    inner: Arc<Mutex<ShellInner>>,
}

#[derive(Default)]
struct ShellInner {
    lv1_snapshot: Option<Lv1StateSnapshot>,
    fade_state: AppFadeState,
    lockout: bool,
    logs: VecDeque<AppLogEntry>,
    next_log_id: u64,
    last_event_at: Option<String>,
}

impl Default for AppFadeState {
    fn default() -> Self {
        Self::Idle
    }
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            handles: Arc::new(Mutex::new(RuntimeHandles::default())),
            inner: Arc::new(Mutex::new(ShellInner::default())),
        }
    }
}

impl ShellState {
    pub async fn snapshot(&self) -> AppSnapshot {
        let inner = self.inner.lock().await;
        snapshot_from_inner(&inner)
    }

    pub async fn set_lockout(&self, enabled: bool) -> AppSnapshot {
        let mut inner = self.inner.lock().await;
        inner.lockout = enabled;
        inner.push_log(LogSource::App, LogSeverity::Info, format!("Lockout {}", if enabled { "enabled" } else { "disabled" }));
        snapshot_from_inner(&inner)
    }

    pub async fn clear_lv1_snapshot(&self) -> AppSnapshot {
        let mut inner = self.inner.lock().await;
        inner.lv1_snapshot = None;
        inner.push_log(LogSource::App, LogSeverity::Info, "Disconnected from LV1".to_string());
        snapshot_from_inner(&inner)
    }
}

impl ShellInner {
    fn push_log(&mut self, source: LogSource, severity: LogSeverity, message: String) {
        self.next_log_id += 1;
        let timestamp = current_timestamp();
        self.last_event_at = Some(timestamp.clone());
        self.logs.push_back(AppLogEntry {
            id: self.next_log_id,
            timestamp,
            source,
            severity,
            message,
        });
        while self.logs.len() > MAX_LOGS {
            self.logs.pop_front();
        }
    }
}

fn snapshot_from_inner(inner: &ShellInner) -> AppSnapshot {
    let connection = inner
        .lv1_snapshot
        .as_ref()
        .map(|snapshot| match snapshot.connection {
            ConnectionStatus::Connecting => AppConnectionState::Connecting,
            ConnectionStatus::Connected => AppConnectionState::Connected,
            ConnectionStatus::Disconnected => AppConnectionState::Disconnected,
        })
        .unwrap_or(AppConnectionState::Disconnected);

    let current_scene = inner.lv1_snapshot.as_ref().and_then(|snapshot| {
        snapshot.scene.as_ref().map(|scene| SceneSummary {
            index: scene.index,
            name: scene.name.clone(),
        })
    });

    let scenes = inner
        .lv1_snapshot
        .as_ref()
        .map(|snapshot| {
            snapshot
                .scene_list
                .iter()
                .map(|scene| SceneSummary {
                    index: scene.index,
                    name: scene.name.clone(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let channel_count = inner
        .lv1_snapshot
        .as_ref()
        .map(|snapshot| snapshot.channels.len())
        .unwrap_or(0);

    AppSnapshot {
        connection,
        current_scene,
        scene_count: scenes.len(),
        scenes,
        channel_count,
        fade_state: inner.fade_state.clone(),
        lockout: inner.lockout,
        logs: inner.logs.iter().cloned().collect(),
        last_event_at: inner.last_event_at.clone(),
    }
}

fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    millis.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lv1_scene_fade_utility::lv1::state::{ChannelInfo, SceneListEntry, SceneState};

    #[tokio::test]
    async fn default_snapshot_is_safe_and_disconnected() {
        let state = ShellState::default();
        let snapshot = state.snapshot().await;

        assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
        assert_eq!(snapshot.current_scene, None);
        assert_eq!(snapshot.scene_count, 0);
        assert_eq!(snapshot.channel_count, 0);
        assert_eq!(snapshot.fade_state, AppFadeState::Idle);
        assert!(!snapshot.lockout);
    }

    #[tokio::test]
    async fn lockout_is_owned_by_rust_state() {
        let state = ShellState::default();
        let snapshot = state.set_lockout(true).await;

        assert!(snapshot.lockout);
        assert_eq!(snapshot.logs.len(), 1);
        assert_eq!(snapshot.logs[0].message, "Lockout enabled");
    }

    #[test]
    fn snapshot_maps_lv1_scene_and_counts() {
        let mut inner = ShellInner::default();
        inner.lv1_snapshot = Some(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(SceneState { index: 3, name: "Verse".to_string() }),
            scene_list: vec![SceneListEntry { index: 3, name: "Verse".to_string() }],
            channels: vec![ChannelInfo { group: 0, channel: 0, name: "Lead".to_string(), gain_db: -6.0, muted: false }],
        });

        let snapshot = snapshot_from_inner(&inner);

        assert_eq!(snapshot.connection, AppConnectionState::Connected);
        assert_eq!(snapshot.current_scene.unwrap().name, "Verse");
        assert_eq!(snapshot.scene_count, 1);
        assert_eq!(snapshot.channel_count, 1);
    }
}
```

- [ ] **Step 4: Create Tauri entrypoint**

Create `src-tauri/src/main.rs`:

```rust
mod app_state;

use app_state::ShellState;

fn main() {
    tauri::Builder::default()
        .manage(ShellState::default())
        .run(tauri::generate_context!())
        .expect("failed to run LV1 Scene Fade Utility");
}
```

- [ ] **Step 5: Run Rust tests for Tauri state**

Run: `cargo test -p lv1-scene-fade-utility-tauri`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/capabilities/default.json src-tauri/build.rs src-tauri/src/main.rs src-tauri/src/app_state.rs Cargo.lock
git commit -m "feat: add tauri shell state"
```

---

## Task 4: Add Tauri Commands And Event Emission

**Files:**
- Modify: `src-tauri/src/app_state.rs`
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Extend state with LV1 snapshot update helpers**

In `src-tauri/src/app_state.rs`, add imports:

```rust
use lv1_scene_fade_utility::lv1::state::{Lv1Event, SceneState};
```

Add these methods inside `impl ShellState`:

```rust
    pub async fn replace_lv1_snapshot(&self, snapshot: Lv1StateSnapshot) -> AppSnapshot {
        let mut inner = self.inner.lock().await;
        inner.lv1_snapshot = Some(snapshot);
        snapshot_from_inner(&inner)
    }

    pub async fn apply_lv1_event(&self, event: &Lv1Event) -> AppSnapshot {
        let mut inner = self.inner.lock().await;
        match event {
            Lv1Event::Connected => {
                inner.push_log(LogSource::Lv1, LogSeverity::Info, "LV1 connected".to_string());
            }
            Lv1Event::Disconnected => {
                inner.lv1_snapshot = None;
                inner.push_log(LogSource::Lv1, LogSeverity::Warning, "LV1 disconnected".to_string());
            }
            Lv1Event::SceneChanged(scene) => {
                ensure_lv1_snapshot(&mut inner).scene = Some(scene.clone());
                inner.push_log(LogSource::Lv1, LogSeverity::Info, format!("Scene changed to {}: {}", scene.index, scene.name));
            }
            Lv1Event::SceneListChanged(scenes) => {
                ensure_lv1_snapshot(&mut inner).scene_list = scenes.clone();
                inner.push_log(LogSource::Lv1, LogSeverity::Info, format!("Scene list updated: {} scenes", scenes.len()));
            }
            Lv1Event::FaderChanged { group, channel, gain_db } => {
                let snapshot = ensure_lv1_snapshot(&mut inner);
                if let Some(existing) = snapshot.channels.iter_mut().find(|ch| ch.group == *group && ch.channel == *channel) {
                    existing.gain_db = *gain_db;
                }
            }
            Lv1Event::MuteChanged { group, channel, muted } => {
                let snapshot = ensure_lv1_snapshot(&mut inner);
                if let Some(existing) = snapshot.channels.iter_mut().find(|ch| ch.group == *group && ch.channel == *channel) {
                    existing.muted = *muted;
                }
            }
            Lv1Event::ChannelTopologyChanged(channels) => {
                ensure_lv1_snapshot(&mut inner).channels = channels.clone();
                inner.push_log(LogSource::Lv1, LogSeverity::Info, format!("Channel topology updated: {} channels", channels.len()));
            }
        }
        snapshot_from_inner(&inner)
    }
```

Add this helper below `impl ShellInner`:

```rust
fn ensure_lv1_snapshot(inner: &mut ShellInner) -> &mut Lv1StateSnapshot {
    inner.lv1_snapshot.get_or_insert_with(|| Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: Vec::new(),
        channels: Vec::new(),
    })
}
```

- [ ] **Step 2: Add tests for event application**

In the `tests` module in `src-tauri/src/app_state.rs`, add:

```rust
    #[tokio::test]
    async fn lv1_scene_event_updates_rust_owned_snapshot() {
        let state = ShellState::default();
        let snapshot = state
            .apply_lv1_event(&Lv1Event::SceneChanged(SceneState {
                index: 7,
                name: "Chorus".to_string(),
            }))
            .await;

        assert_eq!(snapshot.connection, AppConnectionState::Connected);
        assert_eq!(snapshot.current_scene.unwrap().name, "Chorus");
        assert_eq!(snapshot.logs.len(), 1);
    }
```

- [ ] **Step 3: Create command handlers**

Create `src-tauri/src/commands.rs`:

```rust
use lv1_scene_fade_utility::fade::engine::spawn_engine;
use lv1_scene_fade_utility::lv1::discovery::resolve_target;
use lv1_scene_fade_utility::lv1::actor::spawn_actor;
use tauri::{AppHandle, Emitter, State};

use crate::app_state::{AppSnapshot, ShellState};

#[tauri::command]
pub async fn get_app_status(state: State<'_, ShellState>) -> Result<AppSnapshot, String> {
    Ok(state.snapshot().await)
}

#[tauri::command]
pub async fn set_lockout(
    app: AppHandle,
    state: State<'_, ShellState>,
    enabled: bool,
) -> Result<AppSnapshot, String> {
    let snapshot = state.set_lockout(enabled).await;
    emit_snapshot(&app, &snapshot)?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn disconnect_lv1(app: AppHandle, state: State<'_, ShellState>) -> Result<AppSnapshot, String> {
    {
        let mut handles = state.handles.lock().await;
        handles.lv1 = None;
        handles.fade = None;
    }
    let snapshot = state.clear_lv1_snapshot().await;
    emit_snapshot(&app, &snapshot)?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn abort_all_fades(state: State<'_, ShellState>) -> Result<(), String> {
    let fade = { state.handles.lock().await.fade.clone() };
    if let Some(fade) = fade {
        fade.abort_all().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn finish_fade_now(state: State<'_, ShellState>) -> Result<(), String> {
    let fade = { state.handles.lock().await.fade.clone() };
    if let Some(fade) = fade {
        fade.finish_now().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn connect_lv1(
    app: AppHandle,
    state: State<'_, ShellState>,
    host: Option<String>,
    port: Option<u16>,
    timeout_ms: Option<u64>,
) -> Result<AppSnapshot, String> {
    let timeout = timeout_ms.unwrap_or(6000);
    let (host, port) = resolve_target(host, port, timeout).map_err(|err| err.to_string())?;
    let lv1 = spawn_actor(host.clone(), port);
    let fade = spawn_engine(lv1.clone());
    let initial_snapshot = lv1.get_state().await;

    {
        let mut handles = state.handles.lock().await;
        handles.lv1 = Some(lv1.clone());
        handles.fade = Some(fade);
    }

    let snapshot = state.replace_lv1_snapshot(initial_snapshot).await;
    emit_snapshot(&app, &snapshot)?;

    let mut events = lv1.subscribe().await;
    let app_for_task = app.clone();
    let state_for_task = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            let snapshot = state_for_task.apply_lv1_event(&event).await;
            let _ = app_for_task.emit("lv1-event", format!("{:?}", event));
            let _ = app_for_task.emit("app-status-changed", &snapshot);
        }
    });

    Ok(snapshot)
}

fn emit_snapshot(app: &AppHandle, snapshot: &AppSnapshot) -> Result<(), String> {
    app.emit("app-status-changed", snapshot)
        .map_err(|err| err.to_string())
}
```

- [ ] **Step 4: Wire commands in Tauri main**

Update `src-tauri/src/main.rs` to:

```rust
mod app_state;
mod commands;

use app_state::ShellState;

fn main() {
    tauri::Builder::default()
        .manage(ShellState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_status,
            commands::connect_lv1,
            commands::disconnect_lv1,
            commands::abort_all_fades,
            commands::finish_fade_now,
            commands::set_lockout,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LV1 Scene Fade Utility");
}
```

- [ ] **Step 5: Run Tauri crate tests and check build**

Run: `cargo test -p lv1-scene-fade-utility-tauri`

Expected: PASS.

Run: `cargo check -p lv1-scene-fade-utility-tauri`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/app_state.rs src-tauri/src/commands.rs src-tauri/src/main.rs Cargo.lock
git commit -m "feat: expose tauri app snapshot commands"
```

---

## Task 5: Connect React To Rust Snapshots Without Frontend App State

**Files:**
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/types.ts`

- [ ] **Step 1: Add frontend command helpers**

Replace `ui/src/types.ts` with:

```ts
export type ConnectionState = "disconnected" | "connecting" | "connected";
export type FadeState = "idle" | "running" | "blocked";
export type LogSource = "app" | "lv1" | "fade";
export type LogSeverity = "info" | "warning" | "error";

export type SceneSummary = {
  index: number;
  name: string;
};

export type AppLogEntry = {
  id: number;
  timestamp: string;
  source: LogSource;
  severity: LogSeverity;
  message: string;
};

export type AppSnapshot = {
  connection: ConnectionState;
  currentScene: SceneSummary | null;
  scenes: SceneSummary[];
  sceneCount: number;
  channelCount: number;
  fadeState: FadeState;
  lockout: boolean;
  logs: AppLogEntry[];
  lastEventAt: string | null;
};

export const disconnectedSnapshot: AppSnapshot = {
  connection: "disconnected",
  currentScene: null,
  scenes: [],
  sceneCount: 0,
  channelCount: 0,
  fadeState: "idle",
  lockout: false,
  logs: [],
  lastEventAt: null,
};
```

- [ ] **Step 2: Replace React app with command-driven snapshot renderer**

Replace `ui/src/App.tsx` with:

```tsx
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import type { AppSnapshot } from "./types";
import { disconnectedSnapshot } from "./types";

type Tab = "connection" | "scene" | "logs";

export default function App() {
  const [activeTab, setActiveTab] = useState<Tab>("connection");
  const [host, setHost] = useState("");
  const [port, setPort] = useState("");
  const [snapshot, setSnapshot] = useState<AppSnapshot>(disconnectedSnapshot);
  const [commandError, setCommandError] = useState<string | null>(null);

  useEffect(() => {
    void refreshSnapshot(setSnapshot, setCommandError);
    const unlistenPromise = listen<AppSnapshot>("app-status-changed", (event) => {
      setSnapshot(event.payload);
    });
    return () => {
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  async function runSnapshotCommand(command: string, args?: Record<string, unknown>) {
    setCommandError(null);
    try {
      const next = await invoke<AppSnapshot>(command, args);
      setSnapshot(next);
    } catch (error) {
      setCommandError(String(error));
      await refreshSnapshot(setSnapshot, setCommandError);
    }
  }

  async function runVoidCommand(command: string) {
    setCommandError(null);
    try {
      await invoke(command);
      await refreshSnapshot(setSnapshot, setCommandError);
    } catch (error) {
      setCommandError(String(error));
    }
  }

  return (
    <main className="min-h-screen bg-slate-950 text-slate-100">
      <header className="border-b border-slate-800 bg-slate-900/80 px-6 py-4">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div>
            <h1 className="text-xl font-semibold">LV1 Scene Fade Utility</h1>
            <p className="text-sm text-slate-400">{snapshot.currentScene ? `Scene ${snapshot.currentScene.index}: ${snapshot.currentScene.name}` : "No LV1 scene selected"}</p>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <StatusBadge label={snapshot.connection} tone={snapshot.connection === "connected" ? "good" : "neutral"} />
            <StatusBadge label={`Fade: ${snapshot.fadeState}`} tone={snapshot.fadeState === "blocked" ? "warning" : "neutral"} />
            <button
              className={snapshot.lockout ? "rounded-full border border-amber-500/60 bg-amber-950 px-3 py-1 text-sm text-amber-100" : "rounded-full border border-slate-700 bg-slate-800 px-3 py-1 text-sm text-slate-200"}
              onClick={() => runSnapshotCommand("set_lockout", { enabled: !snapshot.lockout })}
            >
              {snapshot.lockout ? "Lockout On" : "Lockout Off"}
            </button>
            <button className="rounded-lg border border-slate-700 px-4 py-3 font-semibold text-slate-100 hover:bg-slate-800" onClick={() => runVoidCommand("finish_fade_now")}>Finish Now</button>
            <button className="rounded-lg bg-red-700 px-5 py-3 font-bold text-white shadow-lg shadow-red-950/40 hover:bg-red-600" onClick={() => runVoidCommand("abort_all_fades")}>Abort All</button>
          </div>
        </div>
        {commandError && <p className="mt-3 rounded-lg border border-red-800 bg-red-950 px-3 py-2 text-sm text-red-100">{commandError}</p>}
      </header>

      <nav className="border-b border-slate-800 px-6">
        <div className="flex gap-2">
          <TabButton active={activeTab === "connection"} onClick={() => setActiveTab("connection")}>Connection</TabButton>
          <TabButton active={activeTab === "scene"} onClick={() => setActiveTab("scene")}>Scene</TabButton>
          <TabButton active={activeTab === "logs"} onClick={() => setActiveTab("logs")}>Logs</TabButton>
        </div>
      </nav>

      <section className="p-6">
        {activeTab === "connection" && (
          <ConnectionTab
            host={host}
            port={port}
            snapshot={snapshot}
            setHost={setHost}
            setPort={setPort}
            connect={() => runSnapshotCommand("connect_lv1", { host: host || null, port: port ? Number(port) : null })}
            disconnect={() => runSnapshotCommand("disconnect_lv1")}
          />
        )}
        {activeTab === "scene" && <SceneTab snapshot={snapshot} />}
        {activeTab === "logs" && <LogsTab snapshot={snapshot} />}
      </section>
    </main>
  );
}

async function refreshSnapshot(setSnapshot: (snapshot: AppSnapshot) => void, setCommandError: (message: string | null) => void) {
  try {
    setSnapshot(await invoke<AppSnapshot>("get_app_status"));
  } catch (error) {
    setCommandError(String(error));
  }
}

function TabButton(props: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return <button className={props.active ? "border-b-2 border-cyan-400 px-4 py-3 text-cyan-200" : "px-4 py-3 text-slate-400 hover:text-slate-100"} onClick={props.onClick}>{props.children}</button>;
}

function StatusBadge(props: { label: string; tone: "neutral" | "warning" | "good" }) {
  const tone = props.tone === "warning" ? "border-amber-500/60 bg-amber-950 text-amber-100" : props.tone === "good" ? "border-emerald-500/60 bg-emerald-950 text-emerald-100" : "border-slate-700 bg-slate-800 text-slate-200";
  return <span className={`rounded-full border px-3 py-1 text-sm capitalize ${tone}`}>{props.label}</span>;
}

function ConnectionTab(props: { snapshot: AppSnapshot; host: string; port: string; setHost: (value: string) => void; setPort: (value: string) => void; connect: () => void; disconnect: () => void }) {
  return (
    <div className="grid gap-5 lg:grid-cols-[1fr_1fr]">
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <h2 className="text-lg font-semibold">Connection</h2>
        <div className="mt-4 grid gap-3">
          <label className="grid gap-1 text-sm text-slate-300">Host <input className="rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-slate-100" value={props.host} onChange={(event) => props.setHost(event.target.value)} placeholder="Auto-discover" /></label>
          <label className="grid gap-1 text-sm text-slate-300">Port <input className="rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-slate-100" value={props.port} onChange={(event) => props.setPort(event.target.value)} placeholder="Auto" inputMode="numeric" /></label>
          <div className="flex gap-3">
            <button className="rounded-lg bg-cyan-700 px-4 py-2 font-semibold text-white hover:bg-cyan-600" onClick={props.connect}>Connect</button>
            <button className="rounded-lg border border-slate-700 px-4 py-2 font-semibold text-slate-100 hover:bg-slate-800" onClick={props.disconnect}>Disconnect</button>
          </div>
        </div>
      </section>
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <h2 className="text-lg font-semibold">Status</h2>
        <dl className="mt-4 grid gap-2 text-sm">
          <StatusRow label="Connection" value={props.snapshot.connection} />
          <StatusRow label="Scenes" value={String(props.snapshot.sceneCount)} />
          <StatusRow label="Channels" value={String(props.snapshot.channelCount)} />
          <StatusRow label="Last Event" value={props.snapshot.lastEventAt ?? "None"} />
        </dl>
      </section>
    </div>
  );
}

function SceneTab({ snapshot }: { snapshot: AppSnapshot }) {
  return (
    <div className="grid gap-5 lg:grid-cols-[1fr_1fr]">
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <h2 className="text-lg font-semibold">Current Scene</h2>
        <p className="mt-2 text-slate-300">{snapshot.currentScene ? `${snapshot.currentScene.index}: ${snapshot.currentScene.name}` : "No current scene reported."}</p>
        <p className="mt-4 rounded-lg border border-slate-800 bg-slate-950 p-3 text-sm text-slate-400">Capture and save workflow will be added in the next phase.</p>
      </section>
      <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <h2 className="text-lg font-semibold">Scene List</h2>
        <div className="mt-4 max-h-96 overflow-auto rounded-lg border border-slate-800">
          {snapshot.scenes.length === 0 ? <p className="p-3 text-sm text-slate-400">No scenes loaded.</p> : snapshot.scenes.map((scene) => <div className="border-b border-slate-800 px-3 py-2 text-sm last:border-b-0" key={`${scene.index}-${scene.name}`}>{scene.index}: {scene.name}</div>)}
        </div>
      </section>
    </div>
  );
}

function LogsTab({ snapshot }: { snapshot: AppSnapshot }) {
  return (
    <section className="rounded-xl border border-slate-800 bg-slate-900 p-5">
      <h2 className="text-lg font-semibold">Logs</h2>
      <div className="mt-4 max-h-[34rem] overflow-auto rounded-lg border border-slate-800">
        {snapshot.logs.length === 0 ? <p className="p-3 text-sm text-slate-400">No events yet.</p> : snapshot.logs.map((entry) => <div className="grid grid-cols-[9rem_5rem_1fr] gap-3 border-b border-slate-800 px-3 py-2 text-sm last:border-b-0" key={entry.id}><span className="text-slate-500">{entry.timestamp}</span><span className="uppercase text-slate-400">{entry.source}</span><span>{entry.message}</span></div>)}
      </div>
    </section>
  );
}

function StatusRow({ label, value }: { label: string; value: string }) {
  return <div className="flex justify-between gap-4 border-b border-slate-800 py-2 last:border-b-0"><dt className="text-slate-500">{label}</dt><dd className="text-right text-slate-100">{value}</dd></div>;
}
```

- [ ] **Step 3: Verify frontend against Rust command names**

Run: `npm run typecheck`

Expected: PASS.

Run: `npm run build`

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add ui/src/App.tsx ui/src/types.ts
git commit -m "feat: render rust-owned app snapshots"
```

---

## Task 6: Final Verification And Manual Smoke Notes

**Files:**
- Modify: `docs/superpowers/plans/2026-06-06-phase-6-tauri-thin-shell.md` if verification notes need correction during execution

- [ ] **Step 1: Run full Rust tests**

Run: `cargo test`

Expected: PASS.

- [ ] **Step 2: Run frontend checks**

Run: `npm run typecheck`

Expected: PASS.

Run: `npm run build`

Expected: PASS.

- [ ] **Step 3: Run Tauri compile check**

Run: `cargo check -p lv1-scene-fade-utility-tauri`

Expected: PASS.

- [ ] **Step 4: Launch manual smoke test**

Run: `npm run tauri -- dev`

Expected: Tauri launches the desktop app. If system WebView dependencies are missing, record the exact missing dependency message in the final implementation summary.

- [ ] **Step 5: Smoke test disconnected state**

In the app:

1. Confirm `Connection`, `Scene`, and `Logs` tabs render.
2. Confirm the header shows disconnected state.
3. Toggle lockout and confirm the header changes.
4. Press `Abort All` while disconnected and confirm the app does not crash.
5. Press `Finish Now` while disconnected and confirm the app does not crash.

- [ ] **Step 6: Smoke test LV1 connection if hardware is available**

In the app:

1. Enter LV1 host and port, or leave blank for discovery.
2. Press `Connect`.
3. Confirm connection status changes when LV1 connects.
4. Confirm scene count, current scene, channel count, and logs update.
5. Press `Disconnect` and confirm the shell returns to safe disconnected state.

- [ ] **Step 7: Commit final verification note if any files changed**

If no files changed during verification, do not create an empty commit.

If files changed, run:

```bash
git add <changed-files>
git commit -m "test: verify tauri shell smoke path"
```
