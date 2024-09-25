use crate::{
    bindings::root::{fs::block_device_read, EINVAL, EIO},
    KResult,
};

use super::vfs::DevId;

pub fn make_device(major: u32, minor: u32) -> DevId {
    (major << 8) & 0xff00u32 | minor & 0xffu32
}

pub struct BlockDevice {
    device: DevId,
}

// pub struct BlockDeviceRequest<'lt> {
//     pub sector: u64, // Sector to read from, in 512-byte blocks
//     pub count: u64,  // Number of sectors to read
//     pub buffer: &'lt [Page],
// }

pub struct BlockDeviceRequest<'lt> {
    pub sector: u64, // Sector to read from, in 512-byte blocks
    pub count: u64,  // Number of sectors to read
    pub buffer: &'lt mut [u8],
}

impl BlockDevice {
    pub fn new(device: DevId) -> Self {
        BlockDevice { device }
    }

    pub fn devid(&self) -> DevId {
        self.device
    }

    pub fn read(&self, req: &mut BlockDeviceRequest) -> KResult<()> {
        // // Verify that the buffer is big enough
        // let buffer_size = req.buffer.iter().fold(0, |acc, e| acc + e.len());
        // if buffer_size / 512 < req.count as usize {
        //     return Err(EINVAL);
        // }

        // Verify that the buffer is big enough
        if req.buffer.len() < req.count as usize * 512 {
            return Err(EINVAL);
        }

        let buffer = req.buffer.as_mut_ptr();

        let nread = unsafe {
            block_device_read(
                self.device as u32,
                buffer as *mut _,
                req.buffer.len(),
                req.sector as usize * 512,
                req.count as usize * 512,
            )
        };

        match nread {
            i if i < 0 => return Err(i as u32),
            i if i as u64 == req.count * 512 => Ok(()),
            _ => Err(EIO),
        }
    }
}
