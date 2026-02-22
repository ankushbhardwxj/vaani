/* Vaani UI — Shared JS for onboarding and settings */

// Wait for pywebview bridge to be ready
function ready(fn) {
    if (window.pywebview && window.pywebview.api) {
        fn();
    } else {
        window.addEventListener('pywebviewready', fn);
    }
}

// --- Wizard Navigation ---

let currentStep = 0;
let totalSteps = 0;

function initWizard() {
    const steps = document.querySelectorAll('.step');
    totalSteps = steps.length;
    showStep(0);
}

function showStep(index) {
    const steps = document.querySelectorAll('.step');
    const dots = document.querySelectorAll('.step-dot');

    steps.forEach(function(s, i) {
        s.classList.toggle('active', i === index);
    });

    dots.forEach(function(d, i) {
        d.classList.remove('active', 'completed');
        if (i === index) d.classList.add('active');
        else if (i < index) d.classList.add('completed');
    });

    currentStep = index;

    var prevBtn = document.getElementById('prev-btn');
    var nextBtn = document.getElementById('next-btn');
    if (prevBtn) prevBtn.style.visibility = index === 0 ? 'hidden' : 'visible';
    if (nextBtn) {
        if (index === totalSteps - 1) {
            nextBtn.textContent = 'Finish';
        } else {
            nextBtn.textContent = 'Continue';
        }
    }
}

function nextStep() {
    if (currentStep < totalSteps - 1) {
        showStep(currentStep + 1);
        onStepEnter(currentStep);
    } else {
        finishOnboarding();
    }
}

function prevStep() {
    if (currentStep > 0) {
        showStep(currentStep - 1);
        onStepEnter(currentStep);
    }
}

function onStepEnter(step) {
    var stepEl = document.querySelectorAll('.step')[step];
    if (!stepEl) return;

    var id = stepEl.id;
    if (id === 'step-apikeys') loadApiKeyStatus();
    if (id === 'step-mic') loadMicrophones();
    if (id === 'step-hotkey') loadHotkey();
    if (id === 'step-perms') checkPermissions();
}

async function finishOnboarding() {
    var btn = document.getElementById('next-btn');
    if (btn) {
        btn.disabled = true;
        btn.textContent = 'Finishing...';
    }
    try { stopMicTest(); } catch (e) {}
    try {
        await window.pywebview.api.complete_onboarding();
    } catch (e) {
        console.error('complete_onboarding failed:', e);
    }
    // Use Python-side destroy — window.close() is a no-op in WKWebView
    try {
        await window.pywebview.api.close_window();
    } catch (e) {
        console.error('close_window failed:', e);
    }
}

// --- API Keys ---

async function loadApiKeyStatus() {
    try {
        var status = await window.pywebview.api.get_api_keys_status();
        if (status) {
            setKeyStatus('openai', status.openai);
            setKeyStatus('anthropic', status.anthropic);
        }
    } catch (e) {
        console.error('loadApiKeyStatus failed:', e);
    }
}

function setKeyStatus(provider, configured) {
    var el = document.getElementById(provider + '-status');
    if (!el) return;
    if (configured) {
        el.className = 'status status-ok';
        el.textContent = 'Configured';
    } else {
        el.className = 'status status-missing';
        el.textContent = 'Not set';
    }
}

async function saveApiKey(provider) {
    var input = document.getElementById(provider + '-key');
    var feedback = document.getElementById(provider + '-feedback');
    if (!input || !input.value.trim()) return;

    try {
        var result = await window.pywebview.api.set_api_key(provider, input.value);
        if (result && result.ok) {
            if (feedback) {
                feedback.className = 'feedback success';
                feedback.textContent = 'Key saved';
            }
            input.value = '';
            loadApiKeyStatus();
            setTimeout(function() { if (feedback) feedback.textContent = ''; }, 2000);
        } else {
            if (feedback) {
                feedback.className = 'feedback error';
                feedback.textContent = (result && result.error) || 'Failed to save';
            }
        }
    } catch (e) {
        console.error('saveApiKey failed:', e);
        if (feedback) {
            feedback.className = 'feedback error';
            feedback.textContent = 'Error saving key';
        }
    }
}

// --- Microphone ---

var micTestInterval = null;

async function loadMicrophones() {
    var select = document.getElementById('mic-select');
    if (!select) return;

    try {
        var mics = await window.pywebview.api.list_microphones();
        var config = await window.pywebview.api.get_config();
        var savedDevice = (config && config.microphone_device != null) ? config.microphone_device : '';

        select.innerHTML = '<option value="">Default</option>';
        if (mics && Array.isArray(mics)) {
            mics.forEach(function(mic) {
                var opt = document.createElement('option');
                opt.value = mic.index;
                opt.textContent = mic.name + (mic.is_default ? ' (Default)' : '');
                select.appendChild(opt);
            });
        }
        // Restore saved selection
        select.value = String(savedDevice);
    } catch (e) {
        console.error('loadMicrophones failed:', e);
    }
}

async function onMicSelect(select) {
    // Save the selected mic device index to config
    var val = select.value;
    var deviceIndex = val ? parseInt(val) : null;
    try {
        await window.pywebview.api.save_config({microphone_device: deviceIndex});
    } catch (e) {
        console.error('save mic selection failed:', e);
    }
}

async function startMicTest() {
    try {
        var select = document.getElementById('mic-select');
        var deviceIndex = select && select.value ? parseInt(select.value) : null;

        await window.pywebview.api.start_mic_test(deviceIndex);

        var btn = document.getElementById('mic-test-btn');
        if (btn) {
            btn.textContent = 'Stop Test';
            btn.onclick = stopMicTest;
        }

        micTestInterval = setInterval(async function() {
            try {
                var level = await window.pywebview.api.get_mic_level();
                updateLevelMeter(level || 0);
            } catch (e) {}
        }, 80);
    } catch (e) {
        console.error('startMicTest failed:', e);
    }
}

async function stopMicTest() {
    if (micTestInterval) {
        clearInterval(micTestInterval);
        micTestInterval = null;
    }
    try {
        await window.pywebview.api.stop_mic_test();
    } catch (e) {}
    updateLevelMeter(0);

    var btn = document.getElementById('mic-test-btn');
    if (btn) {
        btn.textContent = 'Test Microphone';
        btn.onclick = startMicTest;
    }
}

function updateLevelMeter(level) {
    var fill = document.querySelector('.level-meter-fill');
    if (!fill) return;
    var pct = Math.min(level * 100, 100);
    fill.style.width = pct + '%';
    fill.classList.toggle('hot', level > 0.8);
}

// --- Hotkey ---

var recordingHotkey = false;
var pressedKeys = new Set();

async function loadHotkey() {
    try {
        var hotkey = await window.pywebview.api.get_hotkey();
        var display = document.getElementById('hotkey-display');
        if (display && hotkey) display.textContent = formatHotkeyDisplay(hotkey);
    } catch (e) {
        console.error('loadHotkey failed:', e);
    }
}

function formatHotkeyDisplay(hotkey) {
    // Handle both "<alt>" and bare "alt" forms
    return hotkey
        .replace(/<cmd>/gi, '\u2318').replace(/\bcmd\b/gi, '\u2318')
        .replace(/<shift>/gi, '\u21E7').replace(/\bshift\b/gi, '\u21E7')
        .replace(/<ctrl>/gi, '\u2303').replace(/\bctrl\b/gi, '\u2303')
        .replace(/<alt>/gi, '\u2325').replace(/\balt\b/gi, '\u2325')
        .replace(/\+/g, ' ')
        .trim();
}

function startHotkeyRecording() {
    recordingHotkey = true;
    pressedKeys.clear();
    var display = document.getElementById('hotkey-display');
    var btn = document.getElementById('hotkey-record-btn');
    if (display) {
        display.textContent = 'Press keys...';
        display.classList.add('recording');
    }
    if (btn) btn.textContent = 'Cancel';

    document.addEventListener('keydown', onHotkeyDown);
    document.addEventListener('keyup', onHotkeyUp);
}

function stopHotkeyRecording() {
    recordingHotkey = false;
    var display = document.getElementById('hotkey-display');
    var btn = document.getElementById('hotkey-record-btn');
    if (display) display.classList.remove('recording');
    if (btn) btn.textContent = 'Change';

    document.removeEventListener('keydown', onHotkeyDown);
    document.removeEventListener('keyup', onHotkeyUp);
}

function onHotkeyDown(e) {
    e.preventDefault();
    var key = mapKeyToHotkeyPart(e);
    if (key) pressedKeys.add(key);

    var display = document.getElementById('hotkey-display');
    if (display) {
        display.textContent = Array.from(pressedKeys).map(function(k) { return formatHotkeyDisplay(k); }).join(' ');
    }
}

async function onHotkeyUp(e) {
    e.preventDefault();
    if (pressedKeys.size === 0) return;

    var hotkeyStr = Array.from(pressedKeys).join('+');
    stopHotkeyRecording();

    try {
        var result = await window.pywebview.api.set_hotkey(hotkeyStr);
        var display = document.getElementById('hotkey-display');
        if (result && (result.ok || !result.error)) {
            if (display) display.textContent = formatHotkeyDisplay(hotkeyStr);
        } else {
            if (display) display.textContent = 'Error: ' + (result && result.error);
            setTimeout(loadHotkey, 2000);
        }
    } catch (e) {
        console.error('set_hotkey failed:', e);
    }
}

function mapKeyToHotkeyPart(e) {
    if (e.key === 'Meta') return '<cmd>';
    if (e.key === 'Shift') return '<shift>';
    if (e.key === 'Control') return '<ctrl>';
    if (e.key === 'Alt') return '<alt>';
    if (e.key === 'Escape') {
        stopHotkeyRecording();
        loadHotkey();
        return null;
    }
    if (e.key === ' ') return 'space';
    if (e.key.length === 1) return e.key.toLowerCase();
    return e.key.toLowerCase();
}

// --- Permissions ---

async function checkPermissions() {
    try {
        var perms = await window.pywebview.api.check_permissions();
        if (perms) {
            updatePermIcon('perm-mic', perms.microphone);
            updatePermIcon('perm-accessibility', perms.accessibility);
        }
    } catch (e) {
        console.error('checkPermissions failed:', e);
    }
}

function updatePermIcon(id, granted) {
    var icon = document.querySelector('#' + id + ' .perm-icon');
    if (!icon) return;
    icon.className = 'perm-icon ' + (granted ? 'ok' : 'missing');
    icon.textContent = granted ? '\u2713' : '!';
}

// --- Settings Tabs ---

function initTabs() {
    var tabs = document.querySelectorAll('.tab');
    tabs.forEach(function(tab) {
        tab.addEventListener('click', function() {
            var target = tab.dataset.tab;
            tabs.forEach(function(t) { t.classList.remove('active'); });
            tab.classList.add('active');
            document.querySelectorAll('.tab-panel').forEach(function(p) {
                p.classList.toggle('active', p.id === target);
            });
            onTabEnter(target);
        });
    });
}

function onTabEnter(tabId) {
    if (tabId === 'tab-general') loadSettingsGeneral();
    if (tabId === 'tab-mic') {
        loadMicrophones();
        loadSettingsMic();
    }
    if (tabId === 'tab-apikeys') loadApiKeyStatus();
    if (tabId === 'tab-about') loadAbout();
}

function applySettingsGeneral(config) {
    if (!config || config.error) return;

    var hotkeyDisplay = document.getElementById('hotkey-display');
    if (hotkeyDisplay) hotkeyDisplay.textContent = formatHotkeyDisplay(config.hotkey);

    var soundsToggle = document.getElementById('sounds-toggle');
    if (soundsToggle) soundsToggle.checked = config.sounds_enabled;

    var loginToggle = document.getElementById('login-toggle');
    if (loginToggle) loginToggle.checked = config.launch_at_login;

    var modeSelect = document.getElementById('mode-select');
    if (modeSelect) modeSelect.value = config.active_mode;
}

async function loadSettingsGeneral() {
    try {
        var config = await window.pywebview.api.get_config();
        applySettingsGeneral(config);
    } catch (e) {
        console.error('loadSettingsGeneral failed:', e);
    }
}

function applySettingsMic(config) {
    if (!config || config.error) return;

    var vadSlider = document.getElementById('vad-slider');
    if (vadSlider) vadSlider.value = config.vad_threshold;

    var vadValue = document.getElementById('vad-value');
    if (vadValue) vadValue.textContent = config.vad_threshold;
}

async function loadSettingsMic() {
    try {
        var config = await window.pywebview.api.get_config();
        applySettingsMic(config);
    } catch (e) {
        console.error('loadSettingsMic failed:', e);
    }
}

async function loadAbout() {
    try {
        var version = await window.pywebview.api.get_version();
        var el = document.getElementById('about-version');
        if (el && version) el.textContent = 'Version ' + version;
    } catch (e) {}
}

async function saveSetting(key, value) {
    try {
        var data = {};
        data[key] = value;
        await window.pywebview.api.save_config(data);
    } catch (e) {
        console.error('saveSetting failed:', e);
    }
}
