# Vaani - Rust/Tauri Rewrite

> Branch: `rust-tauri-rewrite` — clean-room rewrite, zero Python code.

## Project Context
Vaani is a macOS/Linux menu bar app: hold hotkey, speak, release, polished text appears at cursor. This branch rewrites the entire app from scratch in Rust/Tauri. Target audience is Product Managers — zero CLI, zero technical setup.

Read `PLAN.md` for full architecture, phases, and technical decisions.

## Clean Break
- This branch has ZERO dependency on the Python version (`main` branch).
- Do NOT reference, copy, port, or migrate any Python code.
- Do NOT reference pywebview, rumps, PyObjC, or any Python library.
- The frontend is fresh vanilla HTML/CSS/JS built natively for Tauri.
- Prompt text files are written fresh (same concepts, new files).

## Sacred Rules

### Tests Are Immutable
- NEVER modify a test to make it pass. Fix the code, not the test.
- Only change tests when requirements genuinely change (confirmed by user).
- Every new module must ship with unit tests. No exceptions.
- Run `cargo test` after every code change. If tests fail, stop and fix before proceeding.

### Code Quality
- Write senior-engineer-level Rust. No shortcuts, no TODO hacks, no unwrap() in production code.
- Every `unwrap()` or `expect()` must have a comment justifying why it cannot fail.
- Use `thiserror` for all error types. Propagate with `?`. Error messages must be user-readable (PMs will see them).
- No `unsafe` blocks unless absolutely required for FFI. Each `unsafe` block must have a `// SAFETY:` comment.
- Run `cargo clippy -- -D warnings` before considering any module complete.
- Keep functions under 50 lines. Extract when logic gets complex.

### Architecture Discipline
- Single crate in `src-tauri/`. Modules provide encapsulation.
- Platform-specific code behind `#[cfg(target_os = "...")]`. Define traits in `mod.rs`, implement per-platform.
- All external I/O (APIs, filesystem, keychain) goes through traits so tests can mock them.
- State machine transitions are the single source of truth for app behavior. Never bypass `state.rs`.
- `config.rs` owns the canonical `MODES` list. Never duplicate mode lists.
- `error.rs` owns the unified `VaaniError` type. Never create ad-hoc error types.

### Frontend
- Vanilla HTML/CSS/JS. No frameworks (React, Vue, Svelte). No bundlers (Vite, Webpack). No node_modules.
- All Tauri IPC calls go through `ui/js/api.js`. Never call `invoke()` directly from UI code.
- CSS custom properties for theming. Follow system dark/light preference.
- Frontend is stateless — all state lives in Rust backend, fetched via commands.

### Performance
- Single `reqwest::Client` reused for all HTTP calls (connection pooling + HTTP/2).
- Audio callback (cpal) must be lock-free. No allocations, no mutexes in the hot path.
- VAD runs on ONNX Runtime, not torch. Model bundled at `src-tauri/models/silero_vad.onnx`.
- Claude streaming tokens are flushed to cursor every 50ms via enigo typing. Don't wait for full response.

### Dependencies
- Prefer well-maintained crates with >1000 GitHub stars.
- No `tokio` feature bloat — only enable what's used.
- Pin major versions in Cargo.toml.

## Key Commands
```bash
# Build
cargo build --manifest-path src-tauri/Cargo.toml

# Test (run after every change)
cargo test --manifest-path src-tauri/Cargo.toml

# Lint (run before completing any module)
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings

# Format
cargo fmt --manifest-path src-tauri/Cargo.toml

# Run the app (dev mode)
cargo tauri dev

# Build release .dmg
cargo tauri build
```

## File Reference
- `PLAN.md` — Full architecture, phases, and technical decisions
- `src-tauri/src/error.rs` — All error types
- `src-tauri/src/state.rs` — State machine (IDLE/RECORDING/PROCESSING)
- `src-tauri/src/config.rs` — VaaniConfig + MODES constant
- `src-tauri/src/commands.rs` — Tauri IPC commands (JS<->Rust bridge)
- `src-tauri/src/app.rs` — Pipeline orchestrator
- `ui/js/api.js` — Frontend API layer (all Tauri invoke calls)

## What NOT To Do
- Don't reference or copy any Python code from main branch
- Don't add features not in PLAN.md without asking
- Don't create separate library crates (keep it one crate)
- Don't use `println!` — use `tracing::info!`, `tracing::error!`, etc.
- Don't use `std::process::exit()` — propagate errors up
- Don't store secrets in config files — Keychain/libsecret only
- Don't break the streaming paste design by collecting all tokens first
- Don't add JS frameworks, bundlers, or node_modules to the frontend
