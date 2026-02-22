# Vaani

Voice to polished text, right at your cursor.

Vaani is a macOS menu bar app that captures your voice (including whispers), transcribes it with OpenAI Whisper, enhances it with Claude, and pastes the polished text wherever your cursor is.

## Quick Start

```bash
pip install vaani
vaani setup    # Enter your API keys (stored in macOS Keychain)
vaani start    # Launch the menu bar app
```

## How It Works

1. **Hold `Cmd+Shift+;`** and speak (or click the menu bar icon)
2. **Release** the hotkey — Vaani processes your audio
3. **Polished text** appears at your cursor in ~2-4 seconds

```
Your voice → Whisper STT → Claude enhancement → Pasted at cursor
```

## Enhancement Modes

Switch modes from the menu bar dropdown:

| Mode | Description |
|---|---|
| **Cleanup** (default) | Fix grammar, remove fillers, minimal changes |
| **Professional** | Formal rewrite for business communication |
| **Casual** | Friendly, conversational tone |
| **Bullets** | Convert to organized bullet points |

## Requirements

- **macOS** (uses native menu bar, Keychain, clipboard)
- **Python 3.10+**
- **API Keys:** [OpenAI](https://platform.openai.com/api-keys) + [Anthropic](https://console.anthropic.com/settings/keys)

## macOS Permissions

On first run, grant these in System Settings → Privacy & Security:

| Permission | Why |
|---|---|
| **Microphone** | Audio recording (auto-prompted) |
| **Accessibility** | Simulating Cmd+V paste |
| **Input Monitoring** | Global hotkey detection |

## Configuration

Config file: `~/.vaani/config.yaml`

```yaml
hotkey: "<cmd>+<shift>+;"
active_mode: cleanup
sounds_enabled: true
vad_threshold: 0.3          # Lower = more sensitive (good for whispers)
sample_rate: 16000
max_recording_seconds: 600  # 10 minutes
stt_model: gpt-4o-mini-transcribe
llm_model: claude-haiku-4-5-20251001
```

## Custom Prompts

Override prompts by creating files in `~/.vaani/prompts/`:

```
~/.vaani/prompts/
├── system.txt          # Override base system prompt
├── context.txt         # Add personal context (writing style, preferences)
└── modes/
    └── cleanup.txt     # Override cleanup mode prompt
```

## Privacy

- **Audio** is sent to OpenAI for transcription
- **Transcribed text** is sent to Anthropic for enhancement
- **API keys** are stored in macOS Keychain (never in plaintext)
- **Transcription history** is encrypted (AES) in a local SQLite database
- No data is stored on cloud servers beyond API provider retention policies

## Architecture

```
Hotkey press → Mic capture → VAD (trim silence) → Gain normalization
→ Whisper API (transcribe) → Claude Haiku (enhance) → Paste at cursor
```

- **Main thread:** macOS menu bar (rumps)
- **Background threads:** audio recording, API calls, text pasting
- **State machine:** IDLE → RECORDING → PROCESSING → IDLE

## License

MIT
