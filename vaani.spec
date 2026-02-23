# -*- mode: python ; coding: utf-8 -*-
"""PyInstaller spec for Vaani.app standalone macOS bundle."""

import os
import sys
from pathlib import Path

block_cipher = None

# Project root
ROOT = os.path.dirname(os.path.abspath(SPEC))

a = Analysis(
    [os.path.join(ROOT, 'app_entry.py')],
    pathex=[os.path.join(ROOT, 'src')],
    binaries=[],
    datas=[
        (os.path.join(ROOT, 'src', 'vaani', 'prompts'), os.path.join('vaani', 'prompts')),
        *([
            (os.path.join(ROOT, 'assets'), 'assets')
        ] if Path(os.path.join(ROOT, 'assets')).exists() else []),
        *([
            (os.path.join(ROOT, 'src', 'vaani', 'ui', 'web'), os.path.join('vaani', 'ui', 'web'))
        ] if Path(os.path.join(ROOT, 'src', 'vaani', 'ui', 'web')).exists() else []),
    ],
    hiddenimports=[
        'vaani',
        'vaani.main',
        'vaani.audio',
        'vaani.config',
        'vaani.enhance',
        'vaani.hotkey',
        'vaani.menubar',
        'vaani.output',
        'vaani.state',
        'vaani.storage',
        'vaani.transcribe',
        'vaani.ui',
        'vaani.ui.api',
        'vaani.ui.onboarding',
        'vaani.ui.settings',
        'webview',
        'rumps',
        'pynput',
        'pynput.keyboard',
        'pynput.keyboard._darwin',
        'pyautogui',
        'pyautogui._pyautogui_osx',
        'sounddevice',
        'soundfile',
        'keyring',
        'keyring.backends',
        'keyring.backends.macOS',
        'cryptography',
        'yaml',
        'pydantic',
        'pydantic_settings',
        '_sounddevice_data',
        '_soundfile_data',
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[
        'tkinter',
        'matplotlib',
        'PIL',
        'scipy',
        'pandas',
        'IPython',
        'jupyter',
        'notebook',
    ],
    noarchive=False,
)

pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='Vaani',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    console=False,
    target_arch=None,
)

coll = COLLECT(
    exe,
    a.binaries,
    a.zipfiles,
    a.datas,
    strip=False,
    upx=False,
    name='Vaani',
)

app = BUNDLE(
    coll,
    name='Vaani.app',
    icon=None,  # Add a .icns file path here if you have one
    bundle_identifier='com.vaani.app',
    info_plist={
        'CFBundleName': 'Vaani',
        'CFBundleDisplayName': 'Vaani',
        'CFBundleVersion': '0.2.1',
        'CFBundleShortVersionString': '0.2.1',
        'LSUIElement': True,  # Menu bar app — no dock icon
        'NSMicrophoneUsageDescription': 'Vaani needs microphone access to record your voice for transcription.',
        'NSAppleEventsUsageDescription': 'Vaani needs accessibility access to paste text at your cursor.',
    },
)

