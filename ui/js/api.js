/* ============================================================
   Vaani — Tauri IPC Bridge
   Single source of truth for all backend communication.
   Never call invoke() directly from page scripts.
   ============================================================ */

const { invoke } = window.__TAURI__.core;

export const api = {
  // --- Config ---
  getConfig:                 ()                 => invoke('get_config'),
  saveConfig:                (data)             => invoke('save_config_cmd', { data }),

  // --- API Keys ---
  getApiKeysStatus:          ()                 => invoke('get_api_keys_status'),
  setApiKey:                 (provider, key)    => invoke('set_api_key', { provider, key }),

  // --- Microphone ---
  listMicrophones:           ()                 => invoke('list_microphones'),
  startMicTest:              (deviceIndex)      => invoke('start_mic_test', { deviceIndex }),
  getMicLevel:               ()                 => invoke('get_mic_level'),
  stopMicTest:               ()                 => invoke('stop_mic_test'),

  // --- Hotkey ---
  getHotkey:                 ()                 => invoke('get_hotkey'),
  setHotkey:                 (hotkey)           => invoke('set_hotkey', { hotkey }),

  // --- Permissions ---
  checkPermissions:          ()                 => invoke('check_permissions'),
  requestAccessibility:      ()                 => invoke('request_accessibility'),
  openAccessibilitySettings: ()                 => invoke('open_accessibility_settings'),

  // --- Onboarding ---
  completeOnboarding:        ()                 => invoke('complete_onboarding'),

  // --- Utility ---
  getVersion:                ()                 => invoke('get_version'),
  openLogFile:               ()                 => invoke('open_log_file'),
  openConfigDir:             ()                 => invoke('open_config_dir'),
  closeWindow:               ()                 => invoke('close_window'),
};
