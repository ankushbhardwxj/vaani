/* ============================================================
   Vaani — Settings Page Logic
   Stateless frontend: all state lives in Rust backend.
   ============================================================ */

import { api } from './api.js';

// ---- Toast system -----------------------------------------

function showToast(message, type = 'info') {
  const container = document.getElementById('toast-container');
  const toast = document.createElement('div');
  toast.className = `toast ${type}`;
  toast.textContent = message;
  container.appendChild(toast);

  setTimeout(() => {
    toast.classList.add('dismiss');
    toast.addEventListener('animationend', () => toast.remove());
  }, 3000);
}

// ---- Tabs -------------------------------------------------

function initTabs() {
  const buttons = document.querySelectorAll('.tab-btn');
  const panels = document.querySelectorAll('.tab-content');

  buttons.forEach((btn) => {
    btn.addEventListener('click', () => {
      const target = btn.dataset.tab;

      buttons.forEach((b) => b.classList.remove('active'));
      panels.forEach((p) => p.classList.remove('active'));

      btn.classList.add('active');
      document.getElementById(`tab-${target}`).classList.add('active');
    });
  });
}

// ---- Mode selector ----------------------------------------

function initModeSelector(activeMode) {
  const buttons = document.querySelectorAll('.mode-btn');

  function setActive(mode) {
    buttons.forEach((b) => {
      b.classList.toggle('active', b.dataset.mode === mode);
    });
  }

  setActive(activeMode);

  buttons.forEach((btn) => {
    btn.addEventListener('click', async () => {
      const mode = btn.dataset.mode;
      setActive(mode);
      try {
        await api.saveConfig({ active_mode: mode });
      } catch (err) {
        showToast(`Failed to save mode: ${err}`, 'error');
      }
    });
  });
}

// ---- Hotkey -----------------------------------------------

function initHotkey(currentHotkey) {
  const display = document.getElementById('hotkey-display');
  const changeBtn = document.getElementById('hotkey-change-btn');
  const hint = document.getElementById('hotkey-hint');
  let listening = false;

  display.value = currentHotkey || 'Not set';

  changeBtn.addEventListener('click', () => {
    if (listening) return;
    listening = true;
    display.value = '';
    display.placeholder = 'Press keys...';
    hint.classList.remove('hidden');
    changeBtn.disabled = true;

    function onKey(e) {
      e.preventDefault();
      const parts = [];
      if (e.metaKey) parts.push('Cmd');
      if (e.ctrlKey) parts.push('Ctrl');
      if (e.altKey) parts.push('Alt');
      if (e.shiftKey) parts.push('Shift');

      const key = e.key;
      if (!['Meta', 'Control', 'Alt', 'Shift'].includes(key)) {
        parts.push(key.length === 1 ? key.toUpperCase() : key);
      }

      // Require at least one modifier + one key
      const hasModifier = e.metaKey || e.ctrlKey || e.altKey;
      const hasKey = !['Meta', 'Control', 'Alt', 'Shift'].includes(key);

      if (hasModifier && hasKey) {
        const combo = parts.join('+');
        display.value = combo;
        hint.classList.add('hidden');
        changeBtn.disabled = false;
        listening = false;
        document.removeEventListener('keydown', onKey);

        api.setHotkey(combo).catch((err) => {
          showToast(`Failed to set hotkey: ${err}`, 'error');
        });
      }
    }

    document.addEventListener('keydown', onKey);
  });
}

// ---- Microphone -------------------------------------------

let micTestInterval = null;

async function initMicrophone(config) {
  const select = document.getElementById('mic-select');
  const testBtn = document.getElementById('mic-test-btn');
  const stopBtn = document.getElementById('mic-stop-btn');
  const fill = document.getElementById('level-fill');

  // Populate device list
  try {
    const devices = await api.listMicrophones();
    select.innerHTML = '';
    devices.forEach((d, i) => {
      const opt = document.createElement('option');
      opt.value = d.index;
      opt.textContent = d.name;
      if (config.mic_device_index === d.index) opt.selected = true;
      select.appendChild(opt);
    });
    if (devices.length === 0) {
      select.innerHTML = '<option value="">No microphones found</option>';
    }
  } catch (err) {
    select.innerHTML = '<option value="">Error loading devices</option>';
    showToast(`Could not list microphones: ${err}`, 'error');
  }

  // Save mic selection on change
  select.addEventListener('change', async () => {
    try {
      await api.saveConfig({ mic_device_index: parseInt(select.value, 10) });
    } catch (err) {
      showToast(`Failed to save mic selection: ${err}`, 'error');
    }
  });

  // Start test
  testBtn.addEventListener('click', async () => {
    const deviceIndex = parseInt(select.value, 10);
    if (isNaN(deviceIndex)) return;

    try {
      await api.startMicTest(deviceIndex);
      testBtn.classList.add('hidden');
      stopBtn.classList.remove('hidden');

      micTestInterval = setInterval(async () => {
        try {
          const level = await api.getMicLevel();
          const pct = Math.min(100, Math.max(0, level));
          fill.style.width = `${pct}%`;
          fill.classList.toggle('hot', pct > 80);
        } catch {
          // Silently ignore transient read errors during testing
        }
      }, 100);
    } catch (err) {
      showToast(`Mic test failed: ${err}`, 'error');
    }
  });

  // Stop test
  stopBtn.addEventListener('click', () => stopMicTest());
}

function stopMicTest() {
  if (micTestInterval) {
    clearInterval(micTestInterval);
    micTestInterval = null;
  }
  api.stopMicTest().catch(() => {});

  const fill = document.getElementById('level-fill');
  fill.style.width = '0%';
  fill.classList.remove('hot');

  document.getElementById('mic-test-btn').classList.remove('hidden');
  document.getElementById('mic-stop-btn').classList.add('hidden');
}

// ---- API Keys ---------------------------------------------

async function initApiKeys() {
  const openaiStatus = document.getElementById('openai-status');
  const anthropicStatus = document.getElementById('anthropic-status');
  const openaiSave = document.getElementById('openai-save-btn');
  const anthropicSave = document.getElementById('anthropic-save-btn');
  const openaiInput = document.getElementById('openai-key');
  const anthropicInput = document.getElementById('anthropic-key');

  async function refreshStatus() {
    try {
      const status = await api.getApiKeysStatus();
      openaiStatus.className = `status-dot ${status.openai ? 'green' : 'red'}`;
      anthropicStatus.className = `status-dot ${status.anthropic ? 'green' : 'red'}`;
    } catch (err) {
      showToast(`Could not check key status: ${err}`, 'error');
    }
  }

  await refreshStatus();

  openaiSave.addEventListener('click', async () => {
    const key = openaiInput.value.trim();
    if (!key) return;
    try {
      await api.setApiKey('openai', key);
      openaiInput.value = '';
      showToast('OpenAI key saved', 'success');
      await refreshStatus();
    } catch (err) {
      showToast(`Failed to save key: ${err}`, 'error');
    }
  });

  anthropicSave.addEventListener('click', async () => {
    const key = anthropicInput.value.trim();
    if (!key) return;
    try {
      await api.setApiKey('anthropic', key);
      anthropicInput.value = '';
      showToast('Anthropic key saved', 'success');
      await refreshStatus();
    } catch (err) {
      showToast(`Failed to save key: ${err}`, 'error');
    }
  });
}

// ---- About ------------------------------------------------

async function initAbout() {
  try {
    const version = await api.getVersion();
    document.getElementById('about-version').textContent = `Version ${version}`;
    document.getElementById('version-label').textContent = `Vaani v${version}`;
  } catch {
    document.getElementById('about-version').textContent = 'Version unknown';
  }

  document.getElementById('open-config-btn').addEventListener('click', () => {
    api.openConfigDir().catch((err) => showToast(`${err}`, 'error'));
  });

  document.getElementById('open-log-btn').addEventListener('click', () => {
    api.openLogFile().catch((err) => showToast(`${err}`, 'error'));
  });
}

// ---- Init -------------------------------------------------

document.addEventListener('DOMContentLoaded', async () => {
  initTabs();

  try {
    const config = await api.getConfig();

    initModeSelector(config.active_mode || 'professional');
    initHotkey(config.hotkey || '');
    initMicrophone(config);
    initApiKeys();
    initAbout();
  } catch (err) {
    showToast(`Failed to load settings: ${err}`, 'error');
  }
});

// Clean up mic test if window is closed
window.addEventListener('beforeunload', () => {
  if (micTestInterval) stopMicTest();
});
