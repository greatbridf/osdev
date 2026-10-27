use core::cell::UnsafeCell;
use core::ptr::NonNull;
use core::task::{Poll, Waker};

use eonix_mm::address::{Addr as _, PAddr};
use eonix_mm::paging::Folio as _;
use eonix_sync::{Spin, SpinIrq as _};

use super::command_table::CommandTable;
use super::CommandHeader;
use crate::kernel::constants::EIO;
use crate::kernel::mem::FolioOwned;
use crate::KResult;

pub struct CommandList {
    base: NonNull<u8>,
    _page: FolioOwned,
}

unsafe impl Send for CommandList {}
unsafe impl Sync for CommandList {}

pub struct CommandSlot<'a> {
    cmdheader: &'a UnsafeCell<CommandHeader>,
    /// [`Self::control`] might be used in irq handlers.
    control: &'a Spin<SlotControl>,
}

unsafe impl Send for CommandSlot<'_> {}
unsafe impl Sync for CommandSlot<'_> {}

struct SlotControl {
    state: SlotState,
    waker: Option<Waker>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum SlotState {
    Idle,
    Working,
    Finished,
    // TODO: Implement AHCI error handling
    #[allow(unused)]
    Error,
}

impl CommandList {
    fn cmdheaders(&self) -> &[UnsafeCell<CommandHeader>; 32] {
        unsafe { self.base.cast().as_ref() }
    }

    fn controls_ptr(base: NonNull<u8>) -> NonNull<Spin<SlotControl>> {
        // 24 bytes for SlotControl and extra 8 bytes for Spin.
        const_assert_eq!(size_of::<Spin<SlotControl>>(), 32);

        unsafe { base.add(size_of::<UnsafeCell<CommandHeader>>() * 32).cast() }
    }

    fn controls(&self) -> &[Spin<SlotControl>; 32] {
        unsafe { Self::controls_ptr(self.base).cast().as_ref() }
    }

    pub fn cmdlist_base(&self) -> PAddr {
        self._page.start()
    }

    pub fn recv_fis_base(&self) -> PAddr {
        self._page.start()
            + (size_of::<UnsafeCell<CommandHeader>>() + size_of::<Spin<SlotControl>>()) * 32
    }

    pub fn get(&self, index: usize) -> CommandSlot<'_> {
        CommandSlot {
            cmdheader: &self.cmdheaders()[index],
            control: &self.controls()[index],
        }
    }

    pub fn new() -> Self {
        let mut page = FolioOwned::alloc();
        page.as_bytes_mut().fill(0);

        let base = page.get_ptr();

        let controls_ptr = Self::controls_ptr(base);

        for i in 0..32 {
            unsafe {
                controls_ptr.add(i).write(Spin::new(SlotControl {
                    state: SlotState::Idle,
                    waker: None,
                }));
            }
        }

        Self {
            base: page.get_ptr(),
            _page: page,
        }
    }
}

impl Drop for CommandList {
    fn drop(&mut self) {
        let controls_ptr = Self::controls_ptr(self.base);

        for i in 0..32 {
            unsafe {
                controls_ptr.add(i).drop_in_place();
            }
        }
    }
}

impl CommandSlot<'_> {
    pub fn handle_irq(&self) {
        // We are already in the IRQ handler.
        let mut control = self.control.lock();
        assert_eq!(control.state, SlotState::Working);

        let cmdheader = unsafe {
            // SAFETY: The IRQ handler is only called after the command
            //         is finished.
            &mut *self.cmdheader.get()
        };

        // TODO: Check errors.
        cmdheader.bytes_transferred = 0;
        cmdheader.prdt_length = 0;

        control.state = SlotState::Finished;

        if let Some(waker) = control.waker.take() {
            waker.wake();
        }
    }

    pub fn prepare_command(&self, cmdtable: &CommandTable, write: bool) {
        let mut control = self.control.lock_irq();
        assert_eq!(control.state, SlotState::Idle);

        let cmdheader = unsafe {
            // SAFETY: We are in the idle state.
            &mut *self.cmdheader.get()
        };

        cmdheader.first = 0x05; // FIS type

        if write {
            cmdheader.first |= 0x40;
        }

        cmdheader.second = 0x00;

        cmdheader.prdt_length = cmdtable.prdt_len() as u16;
        cmdheader.bytes_transferred = 0;
        cmdheader.command_table_base = cmdtable.base().addr() as u64;

        cmdheader._reserved = [0; 4];

        control.state = SlotState::Working;
    }

    pub async fn wait_finish(&self) -> KResult<()> {
        core::future::poll_fn(|ctx| {
            let mut control = self.control.lock_irq();

            match control.state {
                SlotState::Idle => unreachable!("Poll called in idle state"),
                SlotState::Working => {
                    control.waker = Some(ctx.waker().clone());
                    Poll::Pending
                }
                SlotState::Finished => {
                    control.state = SlotState::Idle;
                    Poll::Ready(Ok(()))
                }
                SlotState::Error => {
                    control.state = SlotState::Idle;

                    // TODO: Report errors.
                    Poll::Ready(Err(EIO))
                }
            }
        })
        .await
    }
}
