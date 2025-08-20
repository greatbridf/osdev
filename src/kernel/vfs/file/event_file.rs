use core::sync::atomic::{AtomicU64, Ordering};

use crate::{
    io::{Buffer, Stream},
    prelude::KResult,
};

pub struct EventFile {
    pub count: AtomicU64,
    non_block: bool,
}

impl EventFile {
    pub fn new(init_val: u64, non_block: bool) -> Self {
        EventFile {
            count: AtomicU64::new(init_val),
            non_block,
        }
    }

    pub async fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        // self.count.fetch_add(Ordering::Relaxed)
        // if self.non_block {
        //     if self.count.load(Ordering::Relaxed) == 0 {
        //         return 0; // Non-blocking read returns 0 if no events
        //     }
        // }
        todo!();
        return Ok(0);
    }

    pub async fn write(&self, stream: &mut dyn Stream) -> KResult<usize> {
        todo!();
        // self.count.fetch_add(value, Ordering::Relaxed);
    }
}
