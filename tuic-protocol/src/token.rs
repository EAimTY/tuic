use seahash::SeaHasher;
use std::hash::Hasher;

pub fn hash(token: &[u8], seed: usize) -> u64 {
    let mut hasher = SeaHasher::new();
    hasher.write(token);
    hasher.write_usize(seed);
    hasher.finish()
}
