use core::sync::atomic::{AtomicU8, Ordering};

use super::{
    block::make_device,
    console::get_console,
    constants::{EEXIST, EIO},
    task::{block_on, ProcessList, Thread},
    terminal::Terminal,
    vfs::{DevId, File, FileType, TerminalFile},
};
use crate::{
    io::{Buffer, Stream, StreamRead},
    prelude::*,
};
use alloc::{
    boxed::Box,
    collections::btree_map::{BTreeMap, Entry},
    sync::Arc,
};
use eonix_sync::AsProof as _;
use posix_types::open::OpenFlags;

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

static CHAR_DEVICES: Spin<BTreeMap<DevId, Arc<CharDevice>>> = Spin::new(BTreeMap::new());

impl CharDevice {
    pub fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        match &self.device {
            CharDeviceType::Terminal(terminal) => block_on(terminal.read(buffer)),
            CharDeviceType::Virtual(device) => device.read(buffer),
        }
    }

    pub fn write(&self, stream: &mut dyn Stream) -> KResult<usize> {
        match &self.device {
            CharDeviceType::Virtual(device) => device.write(stream),
            CharDeviceType::Terminal(terminal) => stream.read_till_end(&mut [0; 128], |data| {
                terminal.write(data);
                Ok(())
            }),
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

    pub fn open(self: &Arc<Self>, flags: OpenFlags) -> KResult<File> {
        Ok(match &self.device {
            CharDeviceType::Terminal(terminal) => {
                let procs = block_on(ProcessList::get().read());
                let current = Thread::current();
                let session = current.process.session(procs.prove());
                // We only set the control terminal if the process is the session leader.
                if session.sid == Thread::current().process.pid {
                    // Silently fail if we can't set the control terminal.
                    dont_check!(block_on(session.set_control_terminal(
                        &terminal,
                        false,
                        procs.prove()
                    )));
                }

                TerminalFile::new(terminal.clone(), flags)
            }
            CharDeviceType::Virtual(_) => File::new(flags, FileType::CharDev(self.clone())),
        })
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

struct RandomDevice(AtomicU8);

impl VirtualCharDevice for RandomDevice {
    fn read(&self, buffer: &mut dyn Buffer) -> KResult<usize> {
        // TODO: Copy from empty page.
        let mut data = self.0.load(Ordering::Relaxed);
        while let false = buffer.fill(&[data])?.should_stop() {
            data = (data as usize * 91 + 114514) as u8;
        }
        self.0.store(data, Ordering::Relaxed);
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
            make_device(1, 8),
            Arc::from("random"),
            CharDeviceType::Virtual(Box::new(RandomDevice(AtomicU8::new(114)))),
        )?;

        Self::register(
            make_device(1, 9),
            Arc::from("urandom"),
            CharDeviceType::Virtual(Box::new(RandomDevice(AtomicU8::new(91)))),
        )?;

        Self::register(
            make_device(5, 1),
            Arc::from("console"),
            CharDeviceType::Virtual(Box::new(ConsoleDevice)),
        )?;

        Ok(())
    }
}
