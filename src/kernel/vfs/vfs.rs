use crate::prelude::*;

use super::DevId;

#[allow(unused_variables)]
pub trait Vfs {
    fn io_blksize(&self) -> usize;
    fn fs_devid(&self) -> DevId;
    fn as_any(&self) -> &dyn Any;
}
