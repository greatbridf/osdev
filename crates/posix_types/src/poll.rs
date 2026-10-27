pub const FDSET_LENGTH: usize = 1024 / (8 * size_of::<usize>());

// TODO: Implement syscall pselect
#[allow(unused)]
pub struct FDSet {
    fds_bits: [usize; FDSET_LENGTH],
}
