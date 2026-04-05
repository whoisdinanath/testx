use std::hash::Hasher;

/// Deterministic hasher (FNV-1a) that produces identical output across Rust versions.
/// `DefaultHasher` is explicitly documented as non-portable across compiler versions.
pub struct StableHasher(u64);

impl StableHasher {
    pub fn new() -> Self {
        Self(0xcbf29ce484222325) // FNV offset basis
    }
}

impl Default for StableHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= byte as u64;
            self.0 = self.0.wrapping_mul(0x100000001b3); // FNV prime
        }
    }
}
