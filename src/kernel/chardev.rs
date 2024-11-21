use alloc::{
    boxed::Box,
    collections::btree_map::{BTreeMap, Entry},
    sync::Arc,
};
use bindings::{EEXIST, EIO};

use crate::{io::Buffer, kernel::console::CONSOLE, prelude::*};

use super::{
    block::make_device,
    terminal::Terminal,
    vfs::{
        file::{File, TerminalFile},
        DevId,
    },
};

use lazy_static::lazy_static;

pub trait VirtualCharDevice: Send + Sync {
    fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize>;
    fn write(&self, data: &[u8]) -> KResult<usize>;
}

pub enum CharDeviceType {
    Terminal(Arc<Terminal>),
    Virtual(Box<dyn VirtualCharDevice>),
}

pub struct CharDevice {
    name: Arc<str>,
    device: CharDeviceType,
}

lazy_static! {
    pub static ref CHAR_DEVICES: Spin<BTreeMap<DevId, Arc<CharDevice>>> =
        Spin::new(BTreeMap::new());
}

impl CharDevice {
    pub fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        match &self.device {
            CharDeviceType::Terminal(terminal) => terminal.read(buffer),
            CharDeviceType::Virtual(device) => device.read(buffer),
        }
    }

    pub fn write(&self, data: &[u8]) -> KResult<usize> {
        match &self.device {
            CharDeviceType::Virtual(device) => device.write(data),
            CharDeviceType::Terminal(terminal) => {
                for &ch in data.iter() {
                    terminal.show_char(ch);
                }
                Ok(data.len())
            }
        }
    }

    pub fn get(devid: DevId) -> Option<Arc<CharDevice>> {
        CHAR_DEVICES.lock().get(&devid).cloned()
    }

    pub fn register(devid: DevId, name: Arc<str>, device: CharDeviceType) -> KResult<()> {
        match CHAR_DEVICES.lock().entry(devid) {
            Entry::Vacant(entry) => {
                entry.insert(Arc::new(CharDevice { name, device }));
                Ok(())
            }
            Entry::Occupied(_) => Err(EEXIST),
        }
    }

    pub fn open(self: &Arc<Self>) -> KResult<Arc<File>> {
        Ok(match &self.device {
            CharDeviceType::Terminal(terminal) => TerminalFile::new(terminal.clone()),
            CharDeviceType::Virtual(_) => Arc::new(File::CharDev(self.clone())),
        })
    }
}

struct NullDevice;
impl VirtualCharDevice for NullDevice {
    fn read(&self, _buffer: &mut dyn Buffer) -> KResult<usize> {
        Ok(0)
    }

    fn write(&self, _data: &[u8]) -> KResult<usize> {
        Ok(_data.len())
    }
}

struct ZeroDevice;
impl VirtualCharDevice for ZeroDevice {
    fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        // TODO: Copy from empty page.
        while let false = buffer.fill(&[0; 16])?.should_stop() {}
        Ok(buffer.wrote())
    }

    fn write(&self, _data: &[u8]) -> KResult<usize> {
        Ok(_data.len())
    }
}

struct ConsoleDevice;
impl VirtualCharDevice for ConsoleDevice {
    fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        match CONSOLE.lock_irq().get_terminal() {
            Some(console) => console.read(buffer),
            None => Err(EIO),
        }
    }

    fn write(&self, data: &[u8]) -> KResult<usize> {
        match CONSOLE.lock_irq().get_terminal() {
            None => Err(EIO),
            Some(console) => {
                for &ch in data.iter() {
                    console.show_char(ch);
                }
                Ok(data.len())
            }
        }
    }
}

impl CharDevice {
    pub fn init() -> KResult<()> {
        Self::register(
            make_device(1, 3),
            Arc::from("null"),
            CharDeviceType::Virtual(Box::new(NullDevice)),
        )?;

        Self::register(
            make_device(1, 5),
            Arc::from("zero"),
            CharDeviceType::Virtual(Box::new(ZeroDevice)),
        )?;

        Self::register(
            make_device(5, 1),
            Arc::from("console"),
            CharDeviceType::Virtual(Box::new(ConsoleDevice)),
        )?;

        Ok(())
    }
}
