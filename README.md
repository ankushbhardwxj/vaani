# Vaani · वाणी

> *वाणी* — Sanskrit for *speech*, *voice*; the goddess of language and learning.

**Voice to polished text, right at your cursor — anywhere on macOS.**

Vaani is a macOS menu bar app that listens when you hold a hotkey, transcribes your speech with OpenAI Whisper, enhances it with Claude AI, and pastes the result directly at your cursor. No switching apps, no copy-pasting — just speak and the text appears.

---

## The Problem

Typing is slow. Dictation on macOS gives you raw, unedited speech dumps — filler words, broken sentences, no punctuation. Third-party dictation tools are either expensive subscriptions, cloud-locked, or produce output that still needs manual cleanup.

Professionals who write a lot — engineers, writers, product managers, support teams — spend significant time translating their thoughts into polished prose. The gap between what you think and what ends up on screen is real friction.

## Why We Built Vaani

We wanted voice to be a first-class input method, not an afterthought. The goal: speak naturally, get back something you'd actually send or commit. Vaani sits silently in your menu bar and activates on a global hotkey — no window to focus, no app to switch to. It works in any text field, terminal, IDE, browser, Slack, email client, or document editor.

The name "Vaani" (वाणी) comes from Sanskrit, meaning *speech* or *voice* — the goddess of language and learning.

---

## Quick Start

```bash
pip install vaani
python -m spacy download en_core_web_sm
vaani start
```

On first launch, Vaani walks you through entering your API keys and granting macOS permissions. Then:

1. **Hold** `Alt` (or your configured hotkey) and speak
2. **Release** — Vaani transcribes and enhances your speech
3. **Polished text** appears at your cursor in ~2–4 seconds

> **API keys needed:** [OpenAI](https://platform.openai.com/api-keys) (transcription) · [Anthropic](https://console.anthropic.com/settings/keys) (enhancement). Keys are stored in macOS Keychain — never written to disk in plaintext.

---

## How It Works

```
Hold hotkey → Mic capture → VAD trims silence → Gain normalization
   → OpenAI Whisper (transcribe) → Claude Haiku (enhance) → Paste at cursor
```

Every step runs in the background. The menu bar icon shows your current state: idle, recording, or processing.

### Pipeline Detail

| Step | Technology | What it does |
|------|-----------|--------------|
| **Audio capture** | sounddevice / PortAudio | Streams mic input at 16kHz mono |
| **Voice activity detection** | Silero VAD (PyTorch) | Strips silence; handles whisper-level audio |
| **Gain normalization** | RMS-based | Amplifies quiet audio so VAD works on whispers |
| **Transcription** | OpenAI Whisper API | Accurate STT across accents and background noise |
| **Enhancement** | Anthropic Claude Haiku | Polishes grammar, tone, and structure |
| **Output** | pynput + pbcopy/pbpaste | Saves clipboard → pastes → restores clipboard |
| **Name formatting** | spaCy NER (en_core_web_sm) | Detects person names, optionally prefixes with @ |

---

## Enhancement Modes

Switch modes from the menu bar dropdown:

| Mode | What it does |
|------|-------------|
| **Cleanup** | Fix grammar and remove filler words with minimal rewrites |
| **Professional** | Formal rewrite for business communication, emails, and docs |
| **Casual** | Friendly, conversational tone for chats and informal writing |
| **Bullets** | Convert your speech into organized bullet points |

---

## Requirements

- **macOS** 12+ (uses native menu bar, Keychain, clipboard APIs)
- **Python 3.10+**
- **API Keys:** [OpenAI](https://platform.openai.com/api-keys) · [Anthropic](https://console.anthropic.com/settings/keys)

---

## macOS Permissions

Grant these in **System Settings → Privacy & Security** on first run:

| Permission | Why |
|-----------|-----|
| **Microphone** | Audio recording (auto-prompted on first use) |
| **Accessibility** | Simulating `Cmd+V` to paste text |
| **Input Monitoring** | Detecting the global hotkey from any app |

---

## Configuration

Config file: `~/.vaani/config.yaml`

```yaml
hotkey: "alt"                   # Global hotkey to hold while speaking
active_mode: professional       # Default enhancement mode
sounds_enabled: true            # Audio feedback on start/stop
vad_threshold: 0.05             # Lower = more sensitive (good for whispers)
sample_rate: 16000              # Audio sample rate (Hz)
max_recording_seconds: 600      # Auto-stop after 10 minutes
stt_model: whisper-1            # OpenAI transcription model
llm_model: claude-haiku-4-5-20251001  # Anthropic enhancement model
microphone_device: null         # null = system default, or set device index
paste_restore_delay_ms: 100     # How long to wait before restoring clipboard
launch_at_login: false          # Start Vaani automatically on login
```

Configuration reloads automatically when the file changes — no restart needed.

---

## Custom Prompts

Override any prompt by creating files in `~/.vaani/prompts/`:

```
~/.vaani/prompts/
├── system.txt          # Override the base system prompt
├── context.txt         # Add personal context (your writing style, name, role)
└── modes/
    ├── cleanup.txt     # Override the cleanup mode prompt
    ├── professional.txt
    ├── casual.txt
    └── bullets.txt
```

User prompts take priority over built-in defaults. Use `context.txt` to tell Vaani about you — your name, your company, common terms you use — so the output matches your voice.

---

## Privacy

| Data | Where it goes | How it's stored |
|------|--------------|----------------|
| **Audio** | Sent to OpenAI for transcription | Not stored by Vaani |
| **Transcribed text** | Sent to Anthropic for enhancement | Not stored by Vaani |
| **API keys** | macOS Keychain only | Never written to disk in plaintext |
| **Transcription history** | Local SQLite database | AES-256 encrypted (Fernet) |

No data is retained on Vaani's servers because there are no Vaani servers. All cloud calls go directly from your machine to OpenAI and Anthropic under your API account, subject to their data retention policies.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Main Thread (macOS requirement)                            │
│  ┌─────────────┐   ┌───────────────────────────────────┐   │
│  │ HotkeyListener│  │ VaaniMenuBar (rumps event loop)   │   │
│  │ (pynput)    │   │ Status icon · Mode selector       │   │
│  └──────┬──────┘   └──────────────────────────────────-┘   │
│         │ on_press / on_release                              │
└─────────┼───────────────────────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────┐
│  StateMachine: IDLE → RECORDING → PROCESSING → IDLE        │
└─────────────────────────────────────────────────────────────┘
          │
          ▼  (daemon threads)
┌──────────────────────────────────────────────────────────┐
│  AudioRecorder → process_audio() → transcribe() → enhance() → paste_text()  │
│  sounddevice     Silero VAD        Whisper API    Claude     pynput           │
│                  + gain norm       + WAV encode   Haiku      + pbcopy         │
└──────────────────────────────────────────────────────────┘
          │
          ▼
┌─────────────────────────────┐
│  HistoryStore (SQLite)      │
│  Fernet-encrypted records   │
└─────────────────────────────┘
```

---

## Development

```bash
git clone https://github.com/ankushbhardwxj/vaani
cd vaani
python -m venv .venv && source .venv/bin/activate
pip install -e ".[test]"
python -m spacy download en_core_web_sm

# Run tests
pytest

# Run in foreground (with live logs)
vaani start --foreground
```

### Running Tests

```bash
pytest                  # Run all tests
pytest -v               # Verbose output
pytest tests/test_audio.py  # Single module
```

---

## License

MIT — see [LICENSE](LICENSE) for details.
