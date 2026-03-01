//! Lock-free single-producer single-consumer ring buffer for audio samples.
//!
//! Designed for passing `f32` audio samples from cpal's real-time audio callback
//! thread (producer) to a processing thread (consumer) without blocking. The
//! write path is entirely lock-free — no mutexes, no allocations.
//!
//! Internally, samples are stored as `AtomicU32` using `f32::to_bits` /
//! `f32::from_bits` for bit-exact round-tripping (including NaN, negative zero,
//! etc.).

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Round `n` up to the next power of two.
///
/// Returns 1 for `n == 0`.
fn next_power_of_two(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    // If already a power of two, return as-is.
    1_usize
        .checked_shl(usize::BITS - (n - 1).leading_zeros())
        .unwrap_or(n)
}

// ── RingBuffer ───────────────────────────────────────────────────────────────

/// A lock-free, single-producer single-consumer ring buffer for `f32` audio
/// samples.
///
/// One slot is always kept empty to distinguish "full" from "empty", so the
/// usable capacity is `capacity - 1` where `capacity` is the internal
/// power-of-two size.
pub struct RingBuffer {
    buffer: Box<[AtomicU32]>,
    /// Always a power of two.
    capacity: usize,
    /// Index of the next slot to write. Modified only by the producer.
    write_pos: AtomicUsize,
    /// Index of the next slot to read. Modified only by the consumer.
    read_pos: AtomicUsize,
}

// SAFETY: RingBuffer is designed for shared access between exactly two threads.
// All fields use atomic operations with appropriate orderings.
unsafe impl Sync for RingBuffer {}
unsafe impl Send for RingBuffer {}

impl RingBuffer {
    /// Create a new ring buffer with *at least* `capacity` usable slots.
    ///
    /// The internal size is rounded up to the next power of two. Because one
    /// slot is reserved to distinguish full from empty, the actual usable
    /// capacity is `internal_capacity - 1`.
    pub fn new(capacity: usize) -> Self {
        // We need capacity + 1 slots because one is wasted to distinguish
        // full from empty. Round that up to a power of two.
        let actual = next_power_of_two(capacity + 1);
        let buffer: Vec<AtomicU32> = (0..actual).map(|_| AtomicU32::new(0)).collect();

        Self {
            buffer: buffer.into_boxed_slice(),
            capacity: actual,
            write_pos: AtomicUsize::new(0),
            read_pos: AtomicUsize::new(0),
        }
    }

    /// Bit-mask for fast modulo: `pos & mask` is equivalent to `pos % capacity`
    /// when `capacity` is a power of two.
    #[inline]
    fn mask(&self) -> usize {
        self.capacity - 1
    }

    /// Push a single audio sample into the buffer.
    ///
    /// Returns `true` on success, `false` if the buffer is full. This method
    /// is lock-free and safe to call from a real-time audio callback.
    pub fn push(&self, sample: f32) -> bool {
        let write = self.write_pos.load(Ordering::Relaxed);
        let read = self.read_pos.load(Ordering::Acquire);

        let next_write = (write + 1) & self.mask();
        if next_write == read {
            // Buffer is full.
            return false;
        }

        // Store the sample bits. Relaxed is fine — the Release on write_pos
        // below ensures the consumer sees this store.
        self.buffer[write & self.mask()].store(sample.to_bits(), Ordering::Relaxed);

        // Publish the new write position. Release ordering ensures the data
        // store above is visible to the consumer before it sees the updated
        // write_pos.
        self.write_pos.store(next_write, Ordering::Release);
        true
    }

    /// Push multiple samples into the buffer.
    ///
    /// Returns the number of samples actually written (may be less than
    /// `samples.len()` if the buffer fills up).
    pub fn push_slice(&self, samples: &[f32]) -> usize {
        let mut written = 0;
        for &sample in samples {
            if !self.push(sample) {
                break;
            }
            written += 1;
        }
        written
    }

    /// Pop a single audio sample from the buffer.
    ///
    /// Returns `None` if the buffer is empty. This method is lock-free.
    pub fn pop(&self) -> Option<f32> {
        let read = self.read_pos.load(Ordering::Relaxed);
        let write = self.write_pos.load(Ordering::Acquire);

        if read == write {
            // Buffer is empty.
            return None;
        }

        // Load the sample bits. Relaxed is fine — the Acquire on write_pos
        // above ensures we see the data the producer stored before publishing
        // write_pos.
        let bits = self.buffer[read & self.mask()].load(Ordering::Relaxed);

        // Advance the read position. Release ordering ensures the producer
        // sees the updated read_pos only after we have finished reading the
        // slot (so it can safely overwrite it).
        let next_read = (read + 1) & self.mask();
        self.read_pos.store(next_read, Ordering::Release);

        Some(f32::from_bits(bits))
    }

    /// Drain all available samples into a `Vec`.
    ///
    /// This is a consumer-side operation. It reads everything currently
    /// available and returns it in order.
    pub fn drain(&self) -> Vec<f32> {
        let count = self.available();
        let mut out = Vec::with_capacity(count);
        while let Some(sample) = self.pop() {
            out.push(sample);
        }
        out
    }

    /// Number of samples currently available to read.
    pub fn available(&self) -> usize {
        let write = self.write_pos.load(Ordering::Acquire);
        let read = self.read_pos.load(Ordering::Acquire);
        (write.wrapping_sub(read)) & self.mask()
    }

    /// Total usable capacity (internal capacity minus the one reserved slot).
    pub fn capacity(&self) -> usize {
        self.capacity - 1
    }

    /// Reset the buffer, discarding all unread samples.
    ///
    /// Only safe to call when neither producer nor consumer are actively
    /// pushing/popping (e.g., between recording sessions).
    pub fn clear(&self) {
        self.write_pos.store(0, Ordering::Release);
        self.read_pos.store(0, Ordering::Release);
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;
    use std::thread;

    #[test]
    fn new_creates_empty_buffer() {
        let rb = RingBuffer::new(64);
        assert_eq!(rb.available(), 0);
    }

    #[test]
    fn push_and_pop_single_sample() {
        let rb = RingBuffer::new(16);
        assert!(rb.push(0.42));
        let val = rb.pop().unwrap();
        assert!((val - 0.42).abs() < f32::EPSILON);
    }

    #[test]
    fn push_and_pop_preserves_value() {
        let rb = RingBuffer::new(16);

        let test_values: &[f32] = &[
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.123_456_79,
            -0.987_654_3,
            f32::MIN,
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ];

        for &original in test_values {
            assert!(rb.push(original));
            let recovered = rb.pop().unwrap();

            if original.is_nan() {
                assert!(recovered.is_nan(), "NaN did not round-trip");
            } else {
                assert_eq!(
                    original.to_bits(),
                    recovered.to_bits(),
                    "Bit-exact mismatch for {original}"
                );
            }
        }
    }

    #[test]
    fn push_to_full_returns_false() {
        let rb = RingBuffer::new(4);
        let cap = rb.capacity();

        for i in 0..cap {
            assert!(rb.push(i as f32), "push #{i} should succeed");
        }

        // Buffer is now full; next push must fail.
        assert!(!rb.push(999.0), "push to full buffer should return false");
    }

    #[test]
    fn pop_from_empty_returns_none() {
        let rb = RingBuffer::new(8);
        assert!(rb.pop().is_none());
    }

    #[test]
    fn push_slice_writes_all() {
        let rb = RingBuffer::new(256);
        let samples: Vec<f32> = (0..100).map(|i| i as f32).collect();

        let written = rb.push_slice(&samples);
        assert_eq!(written, 100);
        assert_eq!(rb.available(), 100);

        for i in 0..100 {
            let val = rb.pop().unwrap();
            assert!((val - i as f32).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn push_slice_partial_when_nearly_full() {
        let rb = RingBuffer::new(8);
        let cap = rb.capacity();

        // Fill all but 2 slots.
        for i in 0..(cap - 2) {
            assert!(rb.push(i as f32));
        }

        // Try to push 5 samples — only 2 should fit.
        let samples = [1.0, 2.0, 3.0, 4.0, 5.0];
        let written = rb.push_slice(&samples);
        assert_eq!(written, 2);
    }

    #[test]
    fn drain_returns_all_available() {
        let rb = RingBuffer::new(256);
        for i in 0..100 {
            assert!(rb.push(i as f32));
        }

        let drained = rb.drain();
        assert_eq!(drained.len(), 100);

        for (i, &val) in drained.iter().enumerate() {
            assert!((val - i as f32).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn drain_empties_buffer() {
        let rb = RingBuffer::new(64);
        for i in 0..30 {
            assert!(rb.push(i as f32));
        }

        let _ = rb.drain();
        assert_eq!(rb.available(), 0);
        assert!(rb.pop().is_none());
    }

    #[test]
    fn clear_resets_buffer() {
        let rb = RingBuffer::new(64);
        for i in 0..20 {
            assert!(rb.push(i as f32));
        }

        assert!(rb.available() > 0);
        rb.clear();
        assert_eq!(rb.available(), 0);
        assert!(rb.pop().is_none());
    }

    #[test]
    fn capacity_is_power_of_two() {
        // Request 100 usable slots. Internal capacity must be a power-of-two
        // >= 101, so the usable capacity will be that power-of-two minus one.
        let rb = RingBuffer::new(100);
        let cap = rb.capacity();
        // Internal capacity is cap + 1, which must be a power of two.
        assert!(
            (cap + 1).is_power_of_two(),
            "Internal capacity {} is not a power of two",
            cap + 1
        );
        // Usable capacity must be at least what was requested.
        assert!(
            cap >= 100,
            "Usable capacity {cap} is less than requested 100"
        );
        // For 100 requested: need >=101 internal => next pow2 is 128.
        assert_eq!(cap, 127);
    }

    #[test]
    fn concurrent_push_pop() {
        const NUM_SAMPLES: usize = 100_000;
        let rb = Arc::new(RingBuffer::new(1024));

        // Producer: push values 1..=NUM_SAMPLES.
        let rb_producer = Arc::clone(&rb);
        let producer = thread::spawn(move || {
            let mut i: usize = 1;
            while i <= NUM_SAMPLES {
                if rb_producer.push(i as f32) {
                    i += 1;
                } else {
                    // Buffer full — yield and retry.
                    thread::yield_now();
                }
            }
        });

        // Consumer: pop all values, accumulate.
        let rb_consumer = Arc::clone(&rb);
        let consumer = thread::spawn(move || {
            let mut received = Vec::with_capacity(NUM_SAMPLES);
            while received.len() < NUM_SAMPLES {
                if let Some(val) = rb_consumer.pop() {
                    received.push(val);
                } else {
                    // Buffer empty — yield and retry.
                    thread::yield_now();
                }
            }
            received
        });

        producer.join().unwrap();
        let received = consumer.join().unwrap();

        assert_eq!(received.len(), NUM_SAMPLES);

        // Verify ordering — values must arrive in order since SPSC.
        for (idx, &val) in received.iter().enumerate() {
            let expected = (idx + 1) as f32;
            assert!(
                (val - expected).abs() < f32::EPSILON,
                "Mismatch at index {idx}: got {val}, expected {expected}"
            );
        }

        // Verify sum as an additional integrity check.
        let expected_sum: f64 = (1..=NUM_SAMPLES).map(|i| i as f64).sum();
        let actual_sum: f64 = received.iter().map(|&v| v as f64).sum();
        assert!(
            (actual_sum - expected_sum).abs() < 1.0,
            "Sum mismatch: expected {expected_sum}, got {actual_sum}"
        );
    }
}
