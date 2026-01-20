use alloc::boxed::Box;
use alloc::collections::btree_map::{BTreeMap, Entry};
use alloc::sync::Arc;

use posix_types::open::OpenFlags;

use super::console::get_console;
use super::constants::{EEXIST, EIO};
use super::task::{block_on, Thread};
use super::terminal::Terminal;
use super::vfs::types::DeviceId;
use super::vfs::{File, FileType, TerminalFile};
use crate::io::{Buffer, Stream, StreamRead};
use crate::prelude::*;

pub trait VirtualCharDevice: Send + Sync {
    fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize>;
    fn write(&self, stream: &mut dyn Stream) -> KResult<usize>;
}

pub enum CharDeviceType {
    Terminal(Arc<Terminal>),
    Virtual(Box<dyn VirtualCharDevice>),
}

#[allow(dead_code)]
pub struct CharDevice {
    name: Arc<str>,
    device: CharDeviceType,
}

static CHAR_DEVICES: Spin<BTreeMap<DeviceId, Arc<CharDevice>>> =
    Spin::new(BTreeMap::new());

impl CharDevice {
    pub fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        match &self.device {
            CharDeviceType::Terminal(terminal) => {
                block_on(terminal.read(buffer))
            }
            CharDeviceType::Virtual(device) => device.read(buffer),
        }
    }

    pub fn write(&self, stream: &mut dyn Stream) -> KResult<usize> {
        match &self.device {
            CharDeviceType::Virtual(device) => device.write(stream),
            CharDeviceType::Terminal(terminal) => {
                stream.read_till_end(&mut [0; 128], |data| {
                    terminal.write(data);
                    Ok(())
                })
            }
        }
    }

    pub fn get(devid: DeviceId) -> Option<Arc<CharDevice>> {
        CHAR_DEVICES.lock().get(&devid).cloned()
    }

    pub fn register(
        devid: DeviceId,
        name: Arc<str>,
        device: CharDeviceType,
    ) -> KResult<()> {
        match CHAR_DEVICES.lock().entry(devid) {
            Entry::Vacant(entry) => {
                entry.insert(Arc::new(CharDevice { name, device }));
                Ok(())
            }
            Entry::Occupied(_) => Err(EEXIST),
        }
    }

    pub async fn open(
        self: &Arc<Self>,
        thread: &Thread,
        flags: OpenFlags,
    ) -> KResult<File> {
        let file = match &self.device {
            CharDeviceType::Virtual(_) => {
                File::new(flags, FileType::CharDev(self.clone()))
            }
            CharDeviceType::Terminal(terminal) => {
                TerminalFile::open(thread, terminal, flags).await
            }
        };

        Ok(file)
    }
}

struct NullDevice;
impl VirtualCharDevice for NullDevice {
    fn read(&self, _buffer: &mut dyn Buffer) -> KResult<usize> {
        Ok(0)
    }

    fn write(&self, stream: &mut dyn Stream) -> KResult<usize> {
        stream.ignore_all()
    }
}

struct ZeroDevice;
impl VirtualCharDevice for ZeroDevice {
    fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        // TODO: Copy from empty page.
        while let false = buffer.fill(&[0; 16])?.should_stop() {}
        Ok(buffer.wrote())
    }

    fn write(&self, stream: &mut dyn Stream) -> KResult<usize> {
        stream.ignore_all()
    }
}

struct ConsoleDevice;
impl VirtualCharDevice for ConsoleDevice {
    fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        let console_terminal = get_console().ok_or(EIO)?;
        block_on(console_terminal.read(buffer))
    }

    fn write(&self, stream: &mut dyn Stream) -> KResult<usize> {
        let console_terminal = get_console().ok_or(EIO)?;
        stream.read_till_end(&mut [0; 128], |data| {
            console_terminal.write(data);
            Ok(())
        })
    }
}

impl CharDevice {
    pub fn init() -> KResult<()> {
        Self::register(
            DeviceId::new(1, 3),
            Arc::from("null"),
            CharDeviceType::Virtual(Box::new(NullDevice)),
        )?;

        Self::register(
            DeviceId::new(1, 5),
            Arc::from("zero"),
            CharDeviceType::Virtual(Box::new(ZeroDevice)),
        )?;

        Self::register(
            DeviceId::new(5, 1),
            Arc::from("console"),
            CharDeviceType::Virtual(Box::new(ConsoleDevice)),
        )?;

        Ok(())
    }
}
