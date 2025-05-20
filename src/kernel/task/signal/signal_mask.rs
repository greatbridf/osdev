use super::Signal;

#[derive(Debug, Clone, Copy)]
pub struct SignalMask(u64);

impl SignalMask {
    pub(super) const fn new(mask: u64) -> Self {
        Self(mask)
    }

    pub(super) const fn empty() -> Self {
        Self(0)
    }

    pub fn mask(&mut self, mask: Self) {
        self.0 |= mask.0;
    }

    pub fn unmask(&mut self, mask: Self) {
        self.0 &= !mask.0;
    }

    pub fn include(&self, signal: Signal) -> bool {
        let signal_mask = Self::from(signal);
        self.0 & signal_mask.0 != 0
    }
}

impl From<SignalMask> for u64 {
    fn from(value: SignalMask) -> Self {
        let SignalMask(mask) = value;
        mask
    }
}

impl From<u64> for SignalMask {
    fn from(mask: u64) -> Self {
        Self(mask)
    }
}
