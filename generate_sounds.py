"""
Generate soothing, addictive UI sounds for Vaani.

Design:
  - 7-voice super-chorus: center + 3 detuned pairs (±2.5, ±6, ±11 cents)
    — tight pair gives warmth, mid pair gives shimmer, wide pair gives movement
  - Sub-octave layer (freq/2) for bass weight
  - Lower register: D4/F#4 (start), A3/E4 (stop)
  - 35–40 ms ease-in attack — completely gentle
  - Long exponential tail (decay_ratio ~0.60) — sounds linger beautifully
  - 5-tap room reverb with taps up to 130 ms — real sense of space
  - -9 dB headroom — comfortable at any volume

Sounds:
  record_start.wav  — D4 → F#4 ascending major third  (~750 ms)
  record_stop.wav   — A3 + E4 perfect fifth            (~700 ms)
"""

from pathlib import Path

import numpy as np
import soundfile as sf

SR = 44100
OUT_DIR = Path("src/vaani/sounds")
OUT_DIR.mkdir(parents=True, exist_ok=True)


# ---------------------------------------------------------------------------
# Synthesis primitives
# ---------------------------------------------------------------------------

# 7-voice chorus configuration:
#   (detune_cents, relative_gain)
# center voice + 3 symmetric pairs — gains sum to 1.0
CHORUS_VOICES = [
    (  0.0, 0.26),   # center
    ( +2.5, 0.17), (-2.5, 0.17),   # tight pair — warmth
    ( +6.0, 0.12), (-6.0, 0.12),   # mid pair   — shimmer
    (+11.0, 0.08), (-11.0, 0.08),  # wide pair  — movement / "alive" feel
]


def rich_chorus(
    freq: float,
    duration: float,
    amplitude: float = 0.48,
    attack_ms: float = 35.0,
    decay_ratio: float = 0.58,
    sub_gain: float = 0.14,
) -> np.ndarray:
    """
    7-voice super-chorus with sub-octave bass layer.

    Parameters
    ----------
    freq        : fundamental frequency (Hz)
    duration    : total length (seconds)
    amplitude   : peak amplitude after normalisation
    attack_ms   : ease-in ramp length in milliseconds
    decay_ratio : decay time constant as fraction of duration (higher = longer tail)
    sub_gain    : level of the sub-octave (freq/2) bass layer
    """
    n = int(SR * duration)
    t = np.linspace(0, duration, n, endpoint=False)

    # --- 7 chorus voices ---
    wave = np.zeros(n)
    for cents, gain in CHORUS_VOICES:
        f = freq * (2 ** (cents / 1200.0))
        wave += gain * np.sin(2 * np.pi * f * t)

    # --- Sub-octave bass layer ---
    wave += sub_gain * np.sin(2 * np.pi * (freq * 0.5) * t)

    # --- Envelope ---
    tau = duration * decay_ratio
    env = np.exp(-t / tau)

    # Ease-in attack (x^2 curve — smoother than linear)
    a_n = int(attack_ms / 1000.0 * SR)
    ramp = np.linspace(0, 1, a_n) ** 2
    env[:a_n] *= ramp

    wave *= env

    # Normalise to target amplitude
    peak = np.max(np.abs(wave))
    if peak > 0:
        wave = wave / peak * amplitude
    return wave


def add_reverb(signal: np.ndarray, wet: float = 0.28) -> np.ndarray:
    """
    5-tap early-reflection reverb — taps spaced out to 130 ms.

    The longer taps let the tail breathe and feel spacious.
    Each tap is slightly darker (lower gain) to simulate air absorption.
    """
    taps = [
        (18,  0.56),
        (38,  0.44),
        (62,  0.32),
        (91,  0.20),
        (130, 0.11),
    ]
    rev = np.zeros(len(signal))
    for delay_ms, gain in taps:
        d = int(delay_ms / 1000.0 * SR)
        if d < len(signal):
            rev[d:] += gain * signal[: len(signal) - d]
    return signal + wet * rev


def true_stereo(a: np.ndarray, b: np.ndarray,
                pan_a: float = -0.25, pan_b: float = 0.25) -> np.ndarray:
    """Equal-power stereo panning: a → pan_a, b → pan_b."""
    n = max(len(a), len(b))
    a = np.pad(a, (0, n - len(a)))
    b = np.pad(b, (0, n - len(b)))
    left  = (a * np.cos(np.pi / 4 * (1 - pan_a))
             + b * np.cos(np.pi / 4 * (1 - pan_b)))
    right = (a * np.sin(np.pi / 4 * (1 + pan_a))
             + b * np.sin(np.pi / 4 * (1 + pan_b)))
    return np.stack([left, right], axis=1)


def normalize(stereo: np.ndarray, headroom_db: float = -9.0) -> np.ndarray:
    peak = np.max(np.abs(stereo))
    if peak > 0:
        stereo = stereo * (10 ** (headroom_db / 20.0) / peak)
    return stereo


# ---------------------------------------------------------------------------
# Sound 1 — record_start.wav
# D4 (293 Hz) → F#4 (370 Hz): ascending major third, bass-heavy
# F#4 enters at 170 ms — overlap creates a smooth legato transition
# ---------------------------------------------------------------------------

DUR_START = 0.75

d4 = rich_chorus(293.66, DUR_START, amplitude=0.50,
                 attack_ms=35, decay_ratio=0.60, sub_gain=0.16)

fs4_onset = int(0.17 * SR)
fs4_dur   = DUR_START - 0.17
fs4 = rich_chorus(369.99, fs4_dur, amplitude=0.56,
                  attack_ms=40, decay_ratio=0.56, sub_gain=0.13)

total_n   = int(DUR_START * SR)
fs4_pad   = np.pad(fs4, (fs4_onset, total_n - fs4_onset - len(fs4)))[:total_n]

# Reverb applied per-note before mixing → richer spatial image
d4_wet  = add_reverb(d4,      wet=0.26)
fs4_wet = add_reverb(fs4_pad, wet=0.30)

start_stereo = true_stereo(d4_wet, fs4_wet, pan_a=-0.22, pan_b=0.22)
start_stereo = normalize(start_stereo, headroom_db=-9.0)
sf.write(OUT_DIR / "record_start.wav", start_stereo, SR, subtype="PCM_16")
print(f"  ✓  record_start.wav  ({len(start_stereo) / SR * 1000:.0f} ms)")


# ---------------------------------------------------------------------------
# Sound 2 — record_stop.wav
# A3 (220 Hz) + E4 (330 Hz): perfect fifth, deep and settled
# Both voices sound simultaneously — feels conclusive and warm
# Slightly longer decay than start for a satisfying "resting" quality
# ---------------------------------------------------------------------------

DUR_STOP = 0.70

a3 = rich_chorus(220.00, DUR_STOP, amplitude=0.54,
                 attack_ms=38, decay_ratio=0.62, sub_gain=0.18)
e4 = rich_chorus(329.63, DUR_STOP, amplitude=0.36,
                 attack_ms=42, decay_ratio=0.56, sub_gain=0.10)

a3_wet = add_reverb(a3, wet=0.30)
e4_wet = add_reverb(e4, wet=0.22)

stop_stereo = true_stereo(a3_wet, e4_wet, pan_a=-0.18, pan_b=0.18)
stop_stereo = normalize(stop_stereo, headroom_db=-9.0)
sf.write(OUT_DIR / "record_stop.wav", stop_stereo, SR, subtype="PCM_16")
print(f"  ✓  record_stop.wav   ({len(stop_stereo) / SR * 1000:.0f} ms)")

print(f"\nSounds written to {OUT_DIR.resolve()}")
