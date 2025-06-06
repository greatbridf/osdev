use crate::prelude::*;

use super::DevId;

#[allow(dead_code)]
pub trait Vfs: Send + Sync + AsAny {
    fn io_blksize(&self) -> usize;
    fn fs_devid(&self) -> DevId;
    fn is_read_only(&self) -> bool;
}
