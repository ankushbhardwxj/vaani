import { api } from './api.js';

let currentStep = 1;
const totalSteps = 6;
let micTestInterval = null;
let isCapturingHotkey = false;
let currentHotkey = '';

// ─── DOM References ───────────────────────────────────────
const stepNumber = document.getElementById('step-number');
const dots = document.querySelectorAll('.progress-dots .dot');
const btnBack = document.getElementById('btn-back');
const btnSkip = document.getElementById('btn-skip');
const btnNext = document.getElementById('btn-next');
const btnGetStarted = document.getElementById('btn-get-started');

// ─── Initialization ───────────────────────────────────────
document.addEventListener('DOMContentLoaded', async () => {
  showStep(1);
  setupNavigation();
  setupApiKeys();
  setupMicTest();
  setupHotkey();
  setupPermissions();
  setupDone();
});

// ─── Step Navigation ──────────────────────────────────────
function showStep(n) {
  // Clean up previous step if leaving step 3
  if (currentStep === 3 && n !== 3) {
    stopMicTest();
  }

  // Cancel hotkey capture if leaving step 4
  if (currentStep === 4 && n !== 4) {
    cancelHotkeyCapture();
  }

  currentStep = n;

  // Hide all steps, show target
  document.querySelectorAll('.step').forEach(el => {
    el.style.display = 'none';
    el.classList.remove('entering');
  });

  const target = document.getElementById(`step-${n}`);
  target.style.display = 'flex';
  // Trigger entrance animation
  requestAnimationFrame(() => target.classList.add('entering'));

  // Update progress indicator
  stepNumber.textContent = n;
  dots.forEach((dot, i) => {
    dot.classList.remove('active', 'completed');
    if (i + 1 === n) {
      dot.classList.add('active');
    } else if (i + 1 < n) {
      dot.classList.add('completed');
    }
  });

  // Update nav button visibility
  updateNavButtons();

  // Run per-step setup
  onStepEnter(n);
}

function updateNavButtons() {
  const n = currentStep;

  // Step 1: no nav buttons (has Get Started)
  // Step 6: no nav buttons (has Launch Vaani)
  if (n === 1 || n === 6) {
    btnBack.style.display = 'none';
    btnSkip.style.display = 'none';
    btnNext.style.display = 'none';
    return;
  }

  // Back: show on steps 2+
  btnBack.style.display = n > 1 ? '' : 'none';

  // Skip: show on steps 2-5
  btnSkip.style.display = (n >= 2 && n <= 5) ? '' : 'none';

  // Next: show on steps 2-5
  btnNext.style.display = (n >= 2 && n <= 5) ? '' : 'none';

  // Custom next label
  if (n === 5) {
    btnNext.textContent = 'Finish';
  } else {
    btnNext.textContent = 'Next';
  }
}

function setupNavigation() {
  btnGetStarted.addEventListener('click', () => showStep(2));

  btnBack.addEventListener('click', () => {
    if (currentStep > 1) showStep(currentStep - 1);
  });

  btnSkip.addEventListener('click', () => {
    if (currentStep < totalSteps) showStep(currentStep + 1);
  });

  btnNext.addEventListener('click', async () => {
    await onStepNext(currentStep);
  });
}

// ─── Per-Step Enter Logic ─────────────────────────────────
async function onStepEnter(n) {
  try {
    switch (n) {
      case 2:
        await loadApiKeyStatus();
        break;
      case 3:
        await loadMicrophones();
        break;
      case 4:
        await loadCurrentHotkey();
        break;
      case 5:
        await loadPermissions();
        break;
      case 6:
        await loadDoneStep();
        break;
    }
  } catch (err) {
    console.error(`Error entering step ${n}:`, err);
  }
}

// ─── Per-Step Next Logic (save before advancing) ──────────
async function onStepNext(n) {
  try {
    switch (n) {
      case 2:
        await saveApiKeys();
        break;
      case 3:
        stopMicTest();
        break;
    }
  } catch (err) {
    console.error(`Error on next from step ${n}:`, err);
  }

  if (currentStep < totalSteps) {
    showStep(currentStep + 1);
  }
}

// ─── Step 2: API Keys ────────────────────────────────────
const openaiKeyInput = document.getElementById('openai-key');
const anthropicKeyInput = document.getElementById('anthropic-key');
const openaiStatus = document.getElementById('openai-status');
const anthropicStatus = document.getElementById('anthropic-status');
const openaiError = document.getElementById('openai-error');
const anthropicError = document.getElementById('anthropic-error');

function setupApiKeys() {
  // Toggle password visibility
  document.querySelectorAll('.toggle-visibility').forEach(btn => {
    btn.addEventListener('click', () => {
      const targetId = btn.getAttribute('data-target');
      const input = document.getElementById(targetId);
      if (input.type === 'password') {
        input.type = 'text';
      } else {
        input.type = 'password';
      }
    });
  });

  // Prevent get-key-link default
  document.querySelectorAll('.get-key-link').forEach(link => {
    link.addEventListener('click', (e) => e.preventDefault());
  });
}

async function loadApiKeyStatus() {
  openaiError.textContent = '';
  anthropicError.textContent = '';

  try {
    const status = await api.getApiKeysStatus();
    updateKeyStatus(openaiStatus, status.openai);
    updateKeyStatus(anthropicStatus, status.anthropic);
  } catch (err) {
    openaiError.textContent = 'Could not check API key status.';
    console.error('getApiKeysStatus error:', err);
  }
}

function updateKeyStatus(dotEl, isSet) {
  if (isSet) {
    dotEl.classList.add('active');
    dotEl.title = 'Configured';
  } else {
    dotEl.classList.remove('active');
    dotEl.title = 'Not configured';
  }
}

async function saveApiKeys() {
  openaiError.textContent = '';
  anthropicError.textContent = '';

  const openaiKey = openaiKeyInput.value.trim();
  const anthropicKey = anthropicKeyInput.value.trim();

  if (openaiKey) {
    try {
      await api.setApiKey('openai', openaiKey);
      updateKeyStatus(openaiStatus, true);
    } catch (err) {
      openaiError.textContent = 'Failed to save OpenAI key.';
      console.error('setApiKey openai error:', err);
    }
  }

  if (anthropicKey) {
    try {
      await api.setApiKey('anthropic', anthropicKey);
      updateKeyStatus(anthropicStatus, true);
    } catch (err) {
      anthropicError.textContent = 'Failed to save Anthropic key.';
      console.error('setApiKey anthropic error:', err);
    }
  }
}

// ─── Step 3: Microphone Test ──────────────────────────────
const micSelect = document.getElementById('mic-select');
const levelBarFill = document.getElementById('level-bar-fill');
const levelLabel = document.getElementById('level-label');
const btnMicTest = document.getElementById('btn-mic-test');
const btnMicStop = document.getElementById('btn-mic-stop');
const btnSoundsGood = document.getElementById('btn-sounds-good');
const micError = document.getElementById('mic-error');

function setupMicTest() {
  btnMicTest.addEventListener('click', startMicTest);
  btnMicStop.addEventListener('click', () => stopMicTest());
  btnSoundsGood.addEventListener('click', () => {
    stopMicTest();
    showStep(4);
  });
}

async function loadMicrophones() {
  micError.textContent = '';
  resetMicUI();

  try {
    const mics = await api.listMicrophones();
    micSelect.innerHTML = '';

    if (!mics || mics.length === 0) {
      const opt = document.createElement('option');
      opt.value = '';
      opt.textContent = 'No microphones found';
      micSelect.appendChild(opt);
      return;
    }

    mics.forEach((mic, index) => {
      const opt = document.createElement('option');
      opt.value = mic.index !== undefined ? mic.index : index;
      opt.textContent = mic.name || `Microphone ${index + 1}`;
      micSelect.appendChild(opt);
    });
  } catch (err) {
    micError.textContent = 'Could not list microphones.';
    console.error('listMicrophones error:', err);
  }
}

async function startMicTest() {
  micError.textContent = '';
  const deviceIndex = parseInt(micSelect.value, 10);

  if (isNaN(deviceIndex)) {
    micError.textContent = 'Please select a microphone.';
    return;
  }

  try {
    await api.startMicTest(deviceIndex);

    btnMicTest.style.display = 'none';
    btnMicStop.style.display = '';
    btnSoundsGood.style.display = '';
    levelLabel.textContent = 'Listening...';
    levelBarFill.classList.add('active');

    // Poll mic level at 100ms
    micTestInterval = setInterval(async () => {
      try {
        const level = await api.getMicLevel();
        const pct = Math.min(Math.max((level || 0) * 100, 0), 100);
        levelBarFill.style.width = pct + '%';
      } catch (err) {
        console.error('getMicLevel error:', err);
      }
    }, 100);
  } catch (err) {
    micError.textContent = 'Could not start microphone test.';
    console.error('startMicTest error:', err);
  }
}

function stopMicTest() {
  if (micTestInterval) {
    clearInterval(micTestInterval);
    micTestInterval = null;
  }

  // Fire-and-forget stop
  api.stopMicTest().catch(err => {
    console.error('stopMicTest error:', err);
  });

  resetMicUI();
}

function resetMicUI() {
  btnMicTest.style.display = '';
  btnMicStop.style.display = 'none';
  btnSoundsGood.style.display = 'none';
  levelBarFill.style.width = '0%';
  levelBarFill.classList.remove('active');
  levelLabel.textContent = 'Ready to test';
}

// ─── Step 4: Hotkey ───────────────────────────────────────
const hotkeyBadge = document.getElementById('hotkey-badge');
const hotkeyCaptureMsg = document.getElementById('hotkey-capture-msg');
const btnChangeHotkey = document.getElementById('btn-change-hotkey');
const hotkeyError = document.getElementById('hotkey-error');

function setupHotkey() {
  btnChangeHotkey.addEventListener('click', () => {
    startHotkeyCapture();
  });
}

async function loadCurrentHotkey() {
  hotkeyError.textContent = '';
  cancelHotkeyCapture();

  try {
    const hotkey = await api.getHotkey();
    currentHotkey = hotkey || 'Right Option';
    hotkeyBadge.textContent = currentHotkey;
  } catch (err) {
    hotkeyBadge.textContent = '...';
    hotkeyError.textContent = 'Could not load hotkey.';
    console.error('getHotkey error:', err);
  }
}

function startHotkeyCapture() {
  isCapturingHotkey = true;
  hotkeyBadge.textContent = '...';
  hotkeyBadge.classList.add('recording');
  hotkeyCaptureMsg.style.display = '';
  btnChangeHotkey.style.display = 'none';
  hotkeyError.textContent = '';

  document.addEventListener('keydown', handleHotkeyCapture);
}

function cancelHotkeyCapture() {
  if (!isCapturingHotkey) return;
  isCapturingHotkey = false;
  hotkeyBadge.classList.remove('recording');
  hotkeyCaptureMsg.style.display = 'none';
  btnChangeHotkey.style.display = '';
  document.removeEventListener('keydown', handleHotkeyCapture);

  // Restore displayed hotkey
  hotkeyBadge.textContent = currentHotkey || '...';
}

async function handleHotkeyCapture(e) {
  e.preventDefault();
  e.stopPropagation();

  // Build a readable key name
  const keyName = buildKeyName(e);
  if (!keyName) return;

  // Stop capturing
  isCapturingHotkey = false;
  document.removeEventListener('keydown', handleHotkeyCapture);

  hotkeyBadge.textContent = keyName;
  hotkeyBadge.classList.remove('recording');
  hotkeyCaptureMsg.style.display = 'none';
  btnChangeHotkey.style.display = '';

  try {
    await api.setHotkey(keyName);
    currentHotkey = keyName;
  } catch (err) {
    hotkeyError.textContent = 'Failed to save hotkey.';
    console.error('setHotkey error:', err);
  }
}

function buildKeyName(e) {
  const parts = [];
  if (e.metaKey) parts.push('Cmd');
  if (e.ctrlKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Option');
  if (e.shiftKey) parts.push('Shift');

  let key = e.key;
  // Ignore bare modifier keys
  if (['Meta', 'Control', 'Alt', 'Shift'].includes(key)) {
    // If only a modifier was pressed, use it as the hotkey directly
    if (parts.length === 1) return parts[0];
    if (parts.length > 1) return parts.join(' + ');
    return null;
  }

  // Normalize key name
  if (key === ' ') key = 'Space';
  if (key.length === 1) key = key.toUpperCase();

  parts.push(key);
  return parts.join(' + ');
}

// ─── Step 5: Permissions ──────────────────────────────────
const micPermStatus = document.getElementById('mic-perm-status');
const accPermStatus = document.getElementById('acc-perm-status');
const btnGrantMic = document.getElementById('btn-grant-mic');
const btnGrantAcc = document.getElementById('btn-grant-acc');
const btnRefreshPerms = document.getElementById('btn-refresh-perms');
const permError = document.getElementById('perm-error');

function setupPermissions() {
  btnGrantMic.addEventListener('click', async () => {
    // macOS prompts for microphone on first use; we just re-check
    permError.textContent = '';
    try {
      await api.checkPermissions();
      await loadPermissions();
    } catch (err) {
      permError.textContent = 'Could not request microphone access.';
      console.error('grant mic error:', err);
    }
  });

  btnGrantAcc.addEventListener('click', async () => {
    permError.textContent = '';
    try {
      await api.openAccessibilitySettings();
    } catch (err) {
      permError.textContent = 'Could not open Accessibility settings.';
      console.error('openAccessibilitySettings error:', err);
    }
  });

  btnRefreshPerms.addEventListener('click', async () => {
    permError.textContent = '';
    await loadPermissions();
  });
}

async function loadPermissions() {
  permError.textContent = '';

  try {
    const perms = await api.checkPermissions();

    updatePermCard(micPermStatus, btnGrantMic, perms.microphone);
    updatePermCard(accPermStatus, btnGrantAcc, perms.accessibility);
  } catch (err) {
    permError.textContent = 'Could not check permissions.';
    console.error('checkPermissions error:', err);
  }
}

function updatePermCard(dotEl, btnEl, granted) {
  const card = dotEl.closest('.permission-card');

  if (granted) {
    dotEl.classList.add('active');
    dotEl.title = 'Granted';
    card.classList.add('granted');
    btnEl.textContent = 'Granted';
    btnEl.disabled = true;
    btnEl.style.opacity = '0.5';
    btnEl.style.cursor = 'default';
  } else {
    dotEl.classList.remove('active');
    dotEl.title = 'Not granted';
    card.classList.remove('granted');
    btnEl.textContent = 'Grant';
    btnEl.disabled = false;
    btnEl.style.opacity = '';
    btnEl.style.cursor = '';
  }
}

// ─── Step 6: Done ─────────────────────────────────────────
const doneHotkey = document.getElementById('done-hotkey');
const btnLaunch = document.getElementById('btn-launch');

function setupDone() {
  btnLaunch.addEventListener('click', async () => {
    try {
      await api.completeOnboarding();
      await api.closeWindow();
    } catch (err) {
      console.error('Launch error:', err);
    }
  });
}

async function loadDoneStep() {
  // Show the configured hotkey
  try {
    const hotkey = await api.getHotkey();
    currentHotkey = hotkey || currentHotkey || 'Right Option';
    doneHotkey.textContent = currentHotkey;
  } catch (err) {
    doneHotkey.textContent = currentHotkey || '...';
    console.error('getHotkey error:', err);
  }
}
