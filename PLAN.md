# Vaani Rust/Tauri Rewrite Plan

> Branch: `rust-tauri-rewrite`
> Python version lives on `main`. This is a clean-room rewrite — zero code carried from Python.

## Why

The Python version has three adoption-killing problems:
1. **PEP 668** — `pip install vaani` fails on modern macOS/Homebrew Python
2. **CLI setup** — Target users (Product Managers) won't open a terminal
3. **Crash-prone runtime** — PyObjC + rumps + threading = structural instability

## What We're Building

A Tauri v2 desktop app. Rust backend, fresh frontend built natively for Tauri. Distributed as `.dmg` via Homebrew Cask. No Apple Developer Program.

**No Python code is reused.** The Python version on `main` is a separate product. This branch starts from scratch.

## Architecture

```
┌──────────────────────────────────────────────┐
│                  Tauri v2                     │
│  ┌──────────────────────────────────────────┐│
│  │  System Tray (always running)            ││
│  │  ├── Start/Stop Recording                ││
│  │  ├── Mode Selector (5 modes)             ││
│  │  ├── Preferences → Settings Window       ││
│  │  └── Quit                                ││
│  └──────────────────────────────────────────┘│
│  ┌──────────────────────────────────────────┐│
│  │  WebView Windows (on demand)             ││
│  │  ├── Settings (fresh, built for Tauri)   ││
│  │  └── Onboarding (fresh, built for Tauri) ││
│  └──────────────────────────────────────────┘│
│  ┌──────────────────────────────────────────┐│
│  │  Rust Backend                            ││
│  │  ├── Audio: cpal + ring buffer           ││
│  │  ├── VAD: Silero ONNX (ort crate)        ││
│  │  ├── STT: OpenAI Whisper API (reqwest)   ││
│  │  ├── LLM: Claude streaming (reqwest SSE) ││
│  │  ├── Output: enigo typing / arboard clip ││
│  │  ├── Hotkey: rdev (cross-platform)       ││
│  │  ├── Config: serde_yaml (~/.vaani/)      ││
│  │  ├── Secrets: Keychain / libsecret       ││
│  │  ├── History: rusqlite + AES-256-GCM     ││
│  │  └── Prompts: bundled + user override    ││
│  └──────────────────────────────────────────┘│
└──────────────────────────────────────────────┘
```

## Directory Structure

```
vaani/                            # rust-tauri-rewrite branch
├── CLAUDE.md
├── PLAN.md
├── src-tauri/
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── src/
│   │   ├── main.rs               # Entry point
│   │   ├── lib.rs                # Module tree + Tauri command registration
│   │   ├── app.rs                # Pipeline orchestrator
│   │   ├── state.rs              # IDLE/RECORDING/PROCESSING state machine
│   │   ├── config.rs             # VaaniConfig, YAML, MODES constant
│   │   ├── error.rs              # VaaniError (thiserror)
│   │   ├── commands.rs           # #[tauri::command] functions (JS bridge)
│   │   ├── tray.rs               # System tray setup + state-driven updates
│   │   ├── prompts.rs            # Prompt loading (bundled + ~/.vaani/prompts/)
│   │   ├── transcribe.rs         # OpenAI Whisper API
│   │   ├── enhance.rs            # Claude streaming + SSE parsing
│   │   ├── sounds.rs             # Sound playback (rodio)
│   │   ├── audio/
│   │   │   ├── mod.rs
│   │   │   ├── capture.rs        # cpal AudioRecorder
│   │   │   ├── vad.rs            # Silero VAD via ONNX Runtime
│   │   │   ├── processing.rs     # Gain normalization, WAV encoding, VAD trim
│   │   │   └── ring_buffer.rs    # Lock-free ring buffer
│   │   ├── output/
│   │   │   ├── mod.rs
│   │   │   ├── paste.rs          # Streaming paste + clipboard save/restore
│   │   │   └── platform.rs       # Platform-specific Cmd+V / xdotool
│   │   ├── hotkey/
│   │   │   ├── mod.rs            # HotkeyListener trait
│   │   │   ├── macos.rs          # CGEvent tap
│   │   │   └── linux.rs          # evdev/rdev
│   │   ├── keychain/
│   │   │   ├── mod.rs            # SecretStorage trait
│   │   │   ├── macos.rs          # security-framework
│   │   │   └── linux.rs          # secret-service
│   │   ├── storage.rs            # SQLite + AES-256-GCM history
│   │   └── platform/
│   │       ├── mod.rs            # Platform detection + abstraction
│   │       ├── macos.rs          # Accessibility, permissions
│   │       └── linux.rs          # Linux equivalents
│   ├── prompts/                  # Bundled prompt files (written fresh)
│   │   ├── system.txt
│   │   ├── context.txt
│   │   └── modes/
│   │       ├── minimal.txt
│   │       ├── professional.txt
│   │       ├── casual.txt
│   │       ├── code.txt
│   │       └── funny.txt
│   ├── sounds/
│   │   ├── record_start.wav
│   │   └── record_stop.wav
│   ├── models/
│   │   └── silero_vad.onnx       # ~1.8MB (replaces 2GB torch)
│   └── icons/
│       ├── icon.icns
│       ├── icon.ico
│       └── icon.png
├── ui/                           # Frontend (fresh, built for Tauri)
│   ├── index.html                # Router — shows onboarding or settings
│   ├── onboarding.html           # First-run wizard
│   ├── settings.html             # Preferences panel
│   ├── css/
│   │   └── style.css             # Design system: variables, dark/light theme
│   └── js/
│       ├── api.js                # Thin wrapper around Tauri invoke
│       ├── onboarding.js         # Onboarding wizard logic
│       └── settings.js           # Settings panel logic
├── .github/workflows/
│   ├── ci.yml
│   └── release.yml
└── casks/
    └── vaani.rb                  # Homebrew Cask formula
```

## Frontend: Fresh Build for Tauri

**No Python frontend code is reused.** The UI is built from scratch, designed natively for Tauri's IPC.

### Stack
- **Vanilla HTML/CSS/JS** — no frameworks, no build tools, no node_modules
- **Tauri invoke API** directly — `window.__TAURI__.core.invoke()`
- **CSS custom properties** for theming (dark/light, follows system)
- **Zero dependencies** — the frontend ships as static files

### Design Principles
- Clean, minimal UI. No clutter.
- Settings panel: ~600x500px, tabbed (General, Microphone, API Keys, About)
- Onboarding: step-by-step wizard (Welcome, API Keys, Mic Test, Hotkey, Permissions, Done)
- System theme follows macOS/Linux appearance
- All state comes from Rust backend via Tauri commands — frontend is stateless

### JS API Layer (`ui/js/api.js`)
Thin wrapper that all UI code calls. Maps to `#[tauri::command]` functions in Rust:

```js
// api.js — all Tauri IPC calls in one place
const { invoke } = window.__TAURI__.core;

export const api = {
  getConfig:              ()           => invoke('get_config'),
  saveConfig:             (data)       => invoke('save_config', { data }),
  getApiKeysStatus:       ()           => invoke('get_api_keys_status'),
  setApiKey:              (provider, key) => invoke('set_api_key', { provider, key }),
  listMicrophones:        ()           => invoke('list_microphones'),
  startMicTest:           (deviceIndex) => invoke('start_mic_test', { deviceIndex }),
  getMicLevel:            ()           => invoke('get_mic_level'),
  stopMicTest:            ()           => invoke('stop_mic_test'),
  getHotkey:              ()           => invoke('get_hotkey'),
  setHotkey:              (hotkey)     => invoke('set_hotkey', { hotkey }),
  checkPermissions:       ()           => invoke('check_permissions'),
  requestAccessibility:   ()           => invoke('request_accessibility'),
  openAccessibilitySettings: ()       => invoke('open_accessibility_settings'),
  completeOnboarding:     ()           => invoke('complete_onboarding'),
  getVersion:             ()           => invoke('get_version'),
  openLogFile:            ()           => invoke('open_log_file'),
  openConfigDir:          ()           => invoke('open_config_dir'),
  closeWindow:            ()           => invoke('close_window'),
};
```

### Tauri Commands (Rust side — `commands.rs`)
These 18 commands are the complete JS<->Rust contract:

| Command | Returns | Purpose |
|---------|---------|---------|
| `get_config` | `VaaniConfig` | Current config as JSON |
| `save_config` | `()` | Merge and persist config fields |
| `get_api_keys_status` | `{openai: bool, anthropic: bool}` | Which API keys are set |
| `set_api_key` | `()` | Store key in Keychain |
| `list_microphones` | `Vec<{index, name}>` | Available input devices |
| `start_mic_test` | `()` | Begin recording for level meter |
| `get_mic_level` | `f32` | Current RMS level (0.0–1.0) |
| `stop_mic_test` | `()` | Stop test recording |
| `get_hotkey` | `String` | Current hotkey combo |
| `set_hotkey` | `()` | Validate and save new hotkey |
| `check_permissions` | `{mic: bool, accessibility: bool}` | Permission status |
| `request_accessibility` | `bool` | Trigger system prompt, return trust |
| `open_accessibility_settings` | `()` | Open System Settings |
| `complete_onboarding` | `()` | Mark wizard done |
| `get_version` | `String` | App version |
| `open_log_file` | `()` | Open log in default editor |
| `open_config_dir` | `()` | Open ~/.vaani/ in Finder |
| `close_window` | `()` | Close current window |

## Phases

### Phase 1: Foundation + MVP

**Goal**: Working tray app. Hold hotkey -> record -> transcribe via Whisper -> paste raw text at cursor.

**Deliver**:
- `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/build.rs`
- `main.rs`, `lib.rs`, `error.rs`, `state.rs`, `config.rs`
- `audio/capture.rs`, `audio/processing.rs` (WAV encode + gain normalize)
- `transcribe.rs` (Whisper API multipart upload)
- `output/paste.rs` (clipboard swap + Cmd+V)
- `hotkey/mod.rs` + `hotkey/macos.rs` (rdev-based global hotkey)
- `tray.rs` (system tray with Start/Stop + state display)
- `app.rs` (pipeline: capture -> encode -> transcribe -> paste)
- `ui/index.html` (minimal placeholder — just shows "Vaani is running")

**Tests**:
- `state.rs` — all transitions, thread safety, listeners
- `config.rs` — defaults, YAML round-trip, invalid YAML fallback, mode migration
- `audio/processing.rs` — normalize gain, WAV encoding, empty/silent input

**Key Dependencies**:
```toml
tauri = { version = "2", features = ["tray-icon"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
tokio = { version = "1", features = ["full"] }
cpal = "0.15"
reqwest = { version = "0.12", features = ["multipart", "json", "stream"] }
hound = "3.5"
arboard = "3"
enigo = "0.2"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"
rdev = "0.5"
dirs = "5"
```

---

### Phase 2: VAD + Audio Pipeline

**Goal**: Silero VAD trims silence before sending to Whisper. Sound effects on record start/stop.

**Deliver**:
- `audio/vad.rs` — ONNX session, 512-sample chunk inference
- `audio/ring_buffer.rs` — lock-free producer/consumer for cpal callback
- `sounds.rs` — rodio playback
- Bundle `silero_vad.onnx` model (~1.8MB)
- Wire VAD into `audio/processing.rs` pipeline

**Tests**:
- `vad.rs` — mock ONNX session, speech vs silence classification
- Ring buffer — single/multi-producer, overflow handling

**New Dependencies**:
```toml
ort = { version = "2", features = ["download-binaries"] }
ndarray = "0.16"
rodio = "0.19"
```

---

### Phase 3: Claude Enhancement + Streaming Paste

**Goal**: Claude Haiku enhances transcription. Tokens stream to cursor as they arrive.

**Deliver**:
- `enhance.rs` — Anthropic Messages API, SSE stream parsing, token batching
- `prompts.rs` — load system.txt + context.txt + modes/{mode}.txt with user override
- Write fresh prompt files in `src-tauri/prompts/`
- `output/platform.rs` — platform-specific key simulation
- Wire enhancement into `app.rs` pipeline
- Streaming paste: batch tokens every 50ms, type via enigo

**Streaming Paste Design**:
```
Claude SSE -> token buffer -> every 50ms flush -> enigo.text() at cursor
```
Use `enigo.text()` (simulated typing) during streaming to avoid clobbering clipboard.
After stream completes, full text goes to history.

**Tests**:
- `prompts.rs` — user override priority, fallback, mode inclusion
- `enhance.rs` — missing key error, empty input, mocked SSE stream assembly
- Streaming paste — batching timing, empty buffer skip

**New Dependencies**:
```toml
futures = "0.3"
```

---

### Phase 4: Settings UI + Onboarding

**Goal**: Fresh, native Tauri frontend. Settings panel and onboarding wizard.

**Deliver**:
- `ui/css/style.css` — design system with CSS custom properties, dark/light theme
- `ui/js/api.js` — Tauri invoke wrapper (all 18 commands)
- `ui/js/settings.js` — settings panel logic
- `ui/js/onboarding.js` — onboarding wizard logic
- `ui/index.html` — router (shows onboarding if not completed, else minimal)
- `ui/settings.html` — tabbed settings (General, Microphone, API Keys, About)
- `ui/onboarding.html` — step wizard (Welcome, API Keys, Mic Test, Hotkey, Permissions, Done)
- `commands.rs` — all 18 `#[tauri::command]` functions
- Update `tray.rs` with Preferences menu item
- Update `main.rs` to register commands and create windows

**Frontend Design Requirements**:
- Vanilla HTML/CSS/JS. No frameworks, no bundlers, no node_modules.
- System theme detection (prefers-color-scheme)
- Settings: 600x500px, resizable, 4 tabs
- Onboarding: 640x680px, 6 steps with progress indicator
- Real-time mic level meter (canvas or CSS animation)
- Hotkey recorder (capture keypress, display combo)
- API key inputs with save + status indicator
- All async operations show loading state

---

### Phase 5: Keychain + Encrypted History

**Goal**: Secure API key storage, encrypted history DB.

**Deliver**:
- `keychain/mod.rs` — `SecretStorage` trait
- `keychain/macos.rs` — `security-framework` crate
- `keychain/linux.rs` — `secret-service` crate
- `storage.rs` — rusqlite + AES-256-GCM
- Config hot-reload via file mtime checking

**Tests**:
- `storage.rs` — add/retrieve, ordering, limit, encryption, wrong-key handling

**New Dependencies**:
```toml
security-framework = "3"    # cfg(target_os = "macos")
secret-service = "4"         # cfg(target_os = "linux")
rusqlite = { version = "0.32", features = ["bundled"] }
aes-gcm = "0.10"
rand = "0.8"
base64 = "0.22"
```

---

### Phase 6: Distribution + CI/CD

**Goal**: Homebrew Cask, GitHub Actions, auto-update check.

**Deliver**:
- `.github/workflows/ci.yml` — cargo test + clippy + fmt on every push
- `.github/workflows/release.yml` — build .dmg (macOS) + .deb/.AppImage (Linux) on tag
- `casks/vaani.rb` — Homebrew formula with `xattr -rd com.apple.quarantine` postflight
- Startup update check — compare version against GitHub Releases API
- Ad-hoc codesign in CI: `codesign --force --deep --sign -`

---

## Phase Dependencies

```
Phase 1 (MVP) ──→ Phase 2 (VAD) ──→ Phase 3 (Claude + Streaming)
    │                                        │
    │                                        └──→ Phase 4 (Fresh UI)
    │
    └──────────────────────────────────────→ Phase 5 (Keychain + History)
                                                     │
                                                     └──→ Phase 6 (Distribution)
```

Critical path: **Phase 1 → Phase 2 → Phase 3 → Phase 4**

## Key Technical Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Frontend | Vanilla HTML/CSS/JS, fresh build | No Python dependency, no frameworks, no build tools |
| VAD runtime | ONNX Runtime (ort) | 50MB vs 2GB torch |
| Audio I/O | cpal | Standard Rust audio, cross-platform |
| HTTP client | reqwest (HTTP/2, connection pool) | Single client reused for all API calls |
| Streaming paste | enigo.text() during stream, clipboard for final | Don't clobber clipboard mid-generation |
| Config format | YAML (serde_yaml) | Simple, human-readable |
| Encryption | AES-256-GCM (aes-gcm) | NIST standard, fast on AES-NI |
| Global hotkey | rdev | Cross-platform, simpler than raw CGEvent |
| Sound playback | rodio | Pure Rust, no system deps |

## What This Branch Does NOT Contain

- No Python code (.py files)
- No pyproject.toml, setup.py, requirements.txt
- No .venv, no pip, no conda
- No PyObjC, no rumps, no pywebview
- No spaCy, no torch, no numpy
- No reference to Python source files
- No compatibility shims for old code

This is a clean-room rewrite. The Python version lives on `main` and is a separate product.

## Binary Size Budget

| Component | Estimated Size |
|-----------|---------------|
| Rust binary + Tauri runtime | ~15MB |
| ONNX Runtime (ort) | ~50MB |
| silero_vad.onnx model | ~1.8MB |
| SQLite (bundled) | ~3MB |
| Sound files | ~200KB |
| Prompt files | ~5KB |
| Frontend HTML/CSS/JS | ~30KB |
| **Total .app bundle** | **~70MB** |
| **Total .dmg** | **~35MB compressed** |

Down from ~2.5GB Python installation.
