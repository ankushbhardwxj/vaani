use std::fmt;
use std::sync::Mutex;

use crate::error::VaaniError;

/// The three states the Vaani app can be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Idle,
    Recording,
    Processing,
}

impl fmt::Display for AppState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppState::Idle => write!(f, "idle"),
            AppState::Recording => write!(f, "recording"),
            AppState::Processing => write!(f, "processing"),
        }
    }
}

/// A transition listener that is called with (old_state, new_state) on every
/// successful state change.
type Listener = Box<dyn Fn(AppState, AppState) + Send>;

/// Manages Vaani's application state with validated transitions and listener
/// callbacks.
///
/// # Thread Safety
///
/// `StateMachine` is intended to be wrapped in a `Mutex` (or `Arc<Mutex>`) for
/// concurrent access. The struct itself is `Send` but not `Sync` because it
/// holds boxed trait objects.
pub struct StateMachine {
    state: AppState,
    listeners: Vec<Listener>,
}

impl fmt::Debug for StateMachine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateMachine")
            .field("state", &self.state)
            .field(
                "listeners",
                &format!("[{} listener(s)]", self.listeners.len()),
            )
            .finish()
    }
}

impl StateMachine {
    /// Creates a new state machine starting in `Idle`.
    pub fn new() -> Self {
        Self {
            state: AppState::Idle,
            listeners: Vec::new(),
        }
    }

    /// Returns the current state.
    pub fn current(&self) -> AppState {
        self.state
    }

    /// Returns `true` if the current state is `Idle`.
    pub fn is_idle(&self) -> bool {
        self.state == AppState::Idle
    }

    /// Returns `true` if the current state is `Recording`.
    pub fn is_recording(&self) -> bool {
        self.state == AppState::Recording
    }

    /// Returns `true` if the current state is `Processing`.
    pub fn is_processing(&self) -> bool {
        self.state == AppState::Processing
    }

    /// Registers a listener that will be called on every successful state
    /// transition. The listener receives `(old_state, new_state)`.
    ///
    /// If a listener panics, the panic is caught and logged. The state machine
    /// remains in the new state and subsequent listeners still fire.
    pub fn on_transition(&mut self, listener: Listener) {
        self.listeners.push(listener);
    }

    /// Attempts to transition from `Idle` to `Recording`.
    pub fn start_recording(&mut self) -> Result<(), VaaniError> {
        self.transition(AppState::Idle, AppState::Recording, "start recording")
    }

    /// Attempts to transition from `Recording` to `Processing` (stop recording
    /// and begin transcription/enhancement).
    pub fn stop_recording(&mut self) -> Result<(), VaaniError> {
        self.transition(AppState::Recording, AppState::Processing, "stop recording")
    }

    /// Attempts to transition from `Recording` back to `Idle` (cancel without
    /// processing).
    pub fn cancel_recording(&mut self) -> Result<(), VaaniError> {
        self.transition(AppState::Recording, AppState::Idle, "cancel recording")
    }

    /// Attempts to transition from `Processing` back to `Idle` (processing
    /// complete or failed).
    pub fn finish_processing(&mut self) -> Result<(), VaaniError> {
        self.transition(AppState::Processing, AppState::Idle, "finish processing")
    }

    /// Core transition logic. Validates that the current state matches
    /// `expected`, moves to `next`, fires listeners, and logs the transition.
    fn transition(
        &mut self,
        expected: AppState,
        next: AppState,
        action: &str,
    ) -> Result<(), VaaniError> {
        if self.state != expected {
            return Err(VaaniError::InvalidTransition {
                action: action.to_string(),
                state: self.state.to_string(),
            });
        }

        let old = self.state;
        self.state = next;

        tracing::info!(from = %old, to = %next, "state transition: {action}");

        self.notify_listeners(old, next);

        Ok(())
    }

    /// Fires all registered listeners, catching panics so a misbehaving
    /// listener cannot corrupt the state machine.
    fn notify_listeners(&self, old: AppState, new: AppState) {
        for (i, listener) in self.listeners.iter().enumerate() {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener(old, new);
            }));

            if let Err(panic_info) = result {
                let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                tracing::error!(
                    listener_index = i,
                    panic_message = %msg,
                    "state transition listener panicked"
                );
            }
        }
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience type alias for thread-safe shared ownership of a `StateMachine`.
pub type SharedStateMachine = Mutex<StateMachine>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // -----------------------------------------------------------------------
    // 1. Initial state is Idle
    // -----------------------------------------------------------------------
    #[test]
    fn initial_state_is_idle() {
        let sm = StateMachine::new();
        assert_eq!(sm.current(), AppState::Idle);
        assert!(sm.is_idle());
    }

    // -----------------------------------------------------------------------
    // 2. Valid transitions
    // -----------------------------------------------------------------------
    #[test]
    fn valid_transition_idle_to_recording() {
        let mut sm = StateMachine::new();
        sm.start_recording().unwrap();
        assert_eq!(sm.current(), AppState::Recording);
    }

    #[test]
    fn valid_transition_recording_to_processing() {
        let mut sm = StateMachine::new();
        sm.start_recording().unwrap();
        sm.stop_recording().unwrap();
        assert_eq!(sm.current(), AppState::Processing);
    }

    #[test]
    fn valid_transition_processing_to_idle() {
        let mut sm = StateMachine::new();
        sm.start_recording().unwrap();
        sm.stop_recording().unwrap();
        sm.finish_processing().unwrap();
        assert_eq!(sm.current(), AppState::Idle);
    }

    #[test]
    fn valid_transition_recording_to_idle_cancel() {
        let mut sm = StateMachine::new();
        sm.start_recording().unwrap();
        sm.cancel_recording().unwrap();
        assert_eq!(sm.current(), AppState::Idle);
    }

    // -----------------------------------------------------------------------
    // 3. Invalid transitions
    // -----------------------------------------------------------------------
    #[test]
    fn invalid_transition_idle_to_processing() {
        let mut sm = StateMachine::new();
        let err = sm.stop_recording().unwrap_err();
        assert!(err.to_string().contains("Cannot"));
        assert!(err.to_string().contains("idle"));
        assert_eq!(sm.current(), AppState::Idle);
    }

    #[test]
    fn invalid_transition_processing_to_recording() {
        let mut sm = StateMachine::new();
        sm.start_recording().unwrap();
        sm.stop_recording().unwrap();
        let err = sm.start_recording().unwrap_err();
        assert!(err.to_string().contains("processing"));
        assert_eq!(sm.current(), AppState::Processing);
    }

    #[test]
    fn invalid_transition_idle_to_idle() {
        let mut sm = StateMachine::new();
        let err = sm.cancel_recording().unwrap_err();
        assert!(err.to_string().contains("idle"));
        assert_eq!(sm.current(), AppState::Idle);
    }

    #[test]
    fn invalid_transition_recording_to_recording() {
        let mut sm = StateMachine::new();
        sm.start_recording().unwrap();
        let err = sm.start_recording().unwrap_err();
        assert!(err.to_string().contains("recording"));
        assert_eq!(sm.current(), AppState::Recording);
    }

    #[test]
    fn invalid_transition_processing_to_processing() {
        let mut sm = StateMachine::new();
        sm.start_recording().unwrap();
        sm.stop_recording().unwrap();
        let err = sm.stop_recording().unwrap_err();
        assert!(err.to_string().contains("processing"));
        assert_eq!(sm.current(), AppState::Processing);
    }

    // -----------------------------------------------------------------------
    // 4. Helper methods
    // -----------------------------------------------------------------------
    #[test]
    fn helper_methods_reflect_current_state() {
        let mut sm = StateMachine::new();

        // Idle
        assert!(sm.is_idle());
        assert!(!sm.is_recording());
        assert!(!sm.is_processing());

        // Recording
        sm.start_recording().unwrap();
        assert!(!sm.is_idle());
        assert!(sm.is_recording());
        assert!(!sm.is_processing());

        // Processing
        sm.stop_recording().unwrap();
        assert!(!sm.is_idle());
        assert!(!sm.is_recording());
        assert!(sm.is_processing());
    }

    // -----------------------------------------------------------------------
    // 5. Listener fires on valid transition with correct states
    // -----------------------------------------------------------------------
    #[test]
    fn listener_fires_on_valid_transition() {
        let log = Arc::new(Mutex::new(Vec::<(AppState, AppState)>::new()));
        let log_clone = Arc::clone(&log);

        let mut sm = StateMachine::new();
        sm.on_transition(Box::new(move |old, new| {
            log_clone.lock().unwrap().push((old, new));
        }));

        sm.start_recording().unwrap();
        sm.stop_recording().unwrap();
        sm.finish_processing().unwrap();

        let transitions = log.lock().unwrap();
        assert_eq!(transitions.len(), 3);
        assert_eq!(transitions[0], (AppState::Idle, AppState::Recording));
        assert_eq!(transitions[1], (AppState::Recording, AppState::Processing));
        assert_eq!(transitions[2], (AppState::Processing, AppState::Idle));
    }

    // -----------------------------------------------------------------------
    // 6. Listener does NOT fire on invalid transition
    // -----------------------------------------------------------------------
    #[test]
    fn listener_does_not_fire_on_invalid_transition() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let count_clone = Arc::clone(&call_count);

        let mut sm = StateMachine::new();
        sm.on_transition(Box::new(move |_, _| {
            count_clone.fetch_add(1, Ordering::SeqCst);
        }));

        // Attempt several invalid transitions
        let _ = sm.stop_recording();
        let _ = sm.finish_processing();
        let _ = sm.cancel_recording();

        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    // -----------------------------------------------------------------------
    // 7. Panicking listener doesn't break the state machine
    // -----------------------------------------------------------------------
    #[test]
    fn panicking_listener_does_not_corrupt_state() {
        let survived = Arc::new(AtomicUsize::new(0));
        let survived_clone = Arc::clone(&survived);

        let mut sm = StateMachine::new();

        // First listener panics
        sm.on_transition(Box::new(|_, _| {
            panic!("boom");
        }));

        // Second listener should still fire
        sm.on_transition(Box::new(move |_, _| {
            survived_clone.fetch_add(1, Ordering::SeqCst);
        }));

        sm.start_recording().unwrap();

        // State should have advanced despite the panic
        assert_eq!(sm.current(), AppState::Recording);
        // The second listener should have fired
        assert_eq!(survived.load(Ordering::SeqCst), 1);
    }

    // -----------------------------------------------------------------------
    // 8. Full lifecycle
    // -----------------------------------------------------------------------
    #[test]
    fn full_lifecycle() {
        let mut sm = StateMachine::new();

        // First cycle
        sm.start_recording().unwrap();
        sm.stop_recording().unwrap();
        sm.finish_processing().unwrap();
        assert!(sm.is_idle());

        // Second cycle
        sm.start_recording().unwrap();
        sm.stop_recording().unwrap();
        sm.finish_processing().unwrap();
        assert!(sm.is_idle());
    }

    // -----------------------------------------------------------------------
    // 9. Thread safety: 10 threads try idle->recording, exactly one succeeds
    // -----------------------------------------------------------------------
    #[test]
    fn thread_safety_only_one_thread_starts_recording() {
        let sm = Arc::new(Mutex::new(StateMachine::new()));
        let success_count = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(std::sync::Barrier::new(10));

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let sm = Arc::clone(&sm);
                let success_count = Arc::clone(&success_count);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    let mut guard = sm.lock().unwrap();
                    if guard.start_recording().is_ok() {
                        success_count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(success_count.load(Ordering::SeqCst), 1);
        assert_eq!(sm.lock().unwrap().current(), AppState::Recording);
    }

    // -----------------------------------------------------------------------
    // Bonus: Default trait works
    // -----------------------------------------------------------------------
    #[test]
    fn default_creates_idle_state_machine() {
        let sm = StateMachine::default();
        assert!(sm.is_idle());
    }

    // -----------------------------------------------------------------------
    // Bonus: Display for AppState
    // -----------------------------------------------------------------------
    #[test]
    fn app_state_display() {
        assert_eq!(AppState::Idle.to_string(), "idle");
        assert_eq!(AppState::Recording.to_string(), "recording");
        assert_eq!(AppState::Processing.to_string(), "processing");
    }

    // -----------------------------------------------------------------------
    // Bonus: Debug for StateMachine
    // -----------------------------------------------------------------------
    #[test]
    fn state_machine_debug_format() {
        let sm = StateMachine::new();
        let debug = format!("{:?}", sm);
        assert!(debug.contains("Idle"));
        assert!(debug.contains("0 listener(s)"));
    }
}
