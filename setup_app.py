"""py2app build configuration for Vaani.app standalone bundle."""

from setuptools import setup

APP = ["app_entry.py"]
DATA_FILES = [
    ("prompts", [
        "prompts/system.txt",
        "prompts/context.txt",
    ]),
    ("prompts/modes", [
        "prompts/modes/cleanup.txt",
        "prompts/modes/professional.txt",
        "prompts/modes/casual.txt",
        "prompts/modes/bullets.txt",
    ]),
]

OPTIONS = {
    "argv_emulation": False,
    "plist": {
        "CFBundleName": "Vaani",
        "CFBundleDisplayName": "Vaani",
        "CFBundleIdentifier": "com.vaani.app",
        "CFBundleVersion": "0.1.0",
        "CFBundleShortVersionString": "0.1.0",
        "LSUIElement": True,  # Menu bar app (no dock icon)
        "NSMicrophoneUsageDescription": "Vaani needs microphone access to record your voice for transcription.",
        "NSAppleEventsUsageDescription": "Vaani needs accessibility access to paste text at your cursor.",
    },
    "packages": [
        "vaani",
        "openai",
        "anthropic",
        "rumps",
        "pynput",
        "pyautogui",
        "sounddevice",
        "soundfile",
        "numpy",
        "torch",
        "keyring",
        "cryptography",
        "pydantic",
        "pydantic_settings",
        "yaml",
        "click",
        "httpx",
        "httpcore",
        "sniffio",
        "anyio",
        "certifi",
        "h11",
        "idna",
        "distro",
        "jiter",
        "annotated_types",
        "pydantic_core",
        "dotenv",
        "typing_extensions",
    ],
    "includes": [
        "vaani.main",
        "vaani.audio",
        "vaani.config",
        "vaani.enhance",
        "vaani.hotkey",
        "vaani.menubar",
        "vaani.output",
        "vaani.state",
        "vaani.storage",
        "vaani.transcribe",
    ],
    "iconfile": None,  # Add a .icns file here if you have one
}

setup(
    app=APP,
    name="Vaani",
    data_files=DATA_FILES,
    options={"py2app": OPTIONS},
)

