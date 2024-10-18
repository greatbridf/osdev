use core::hash::Hasher;

#[derive(Default)]
pub struct KernelHasher {
    cur: u64,
}

impl Hasher for KernelHasher {
    fn finish(&self) -> u64 {
        self.cur
    }

    fn write(&mut self, bytes: &[u8]) {
        const SEED: u64 = 131;
        for &byte in bytes {
            self.cur = self.cur.wrapping_mul(SEED).wrapping_add(byte as u64)
        }
    }
}
