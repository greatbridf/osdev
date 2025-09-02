use super::{command_table::CommandTable, CommandHeader};
use crate::KResult;
use core::pin::pin;
use eonix_mm::address::Addr as _;
use eonix_sync::{Spin, SpinIrq as _, WaitList};

pub struct CommandSlot<'a> {
    /// # Usage
    /// `inner.cmdheader` might be used in irq handler. So in order to wait for
    /// commands to finish, we should use `lock_irq` on `inner`
    inner: Spin<CommandSlotInner<'a>>,
    wait_list: WaitList,
}

struct CommandSlotInner<'a> {
    state: SlotState,
    cmdheader: &'a mut CommandHeader,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum SlotState {
    Idle,
    Working,
    Finished,
    Error,
}

impl<'a> CommandSlot<'a> {
    pub fn new(cmdheader: &'a mut CommandHeader) -> Self {
        Self {
            inner: Spin::new(CommandSlotInner {
                state: SlotState::Idle,
                cmdheader,
            }),
            wait_list: WaitList::new(),
        }
    }

    pub fn handle_irq(&self) {
        let mut inner = self.inner.lock();
        debug_assert_eq!(inner.state, SlotState::Working);

        // TODO: Check errors.
        inner.state = SlotState::Finished;
        inner.cmdheader.bytes_transferred = 0;
        inner.cmdheader.prdt_length = 0;

        self.wait_list.notify_all();
    }

    pub fn prepare_command(&self, cmdtable: &CommandTable, write: bool) {
        let mut inner = self.inner.lock_irq();
        let cmdheader = &mut inner.cmdheader;

        cmdheader.first = 0x05; // FIS type

        if write {
            cmdheader.first |= 0x40;
        }

        cmdheader.second = 0x00;

        cmdheader.prdt_length = cmdtable.prdt_len();
        cmdheader.bytes_transferred = 0;
        cmdheader.command_table_base = cmdtable.base().addr() as u64;

        cmdheader._reserved = [0; 4];

        inner.state = SlotState::Working;
    }

    pub async fn wait_finish(&self) -> KResult<()> {
        let mut inner = loop {
            let mut wait = pin!(self.wait_list.prepare_to_wait());

            {
                let inner = self.inner.lock_irq();
                if inner.state != SlotState::Working {
                    break inner;
                }
                wait.as_mut().add_to_wait_list();

                if inner.state != SlotState::Working {
                    break inner;
                }
            }

            wait.await;
        };

        inner.state = SlotState::Idle;

        Ok(())
    }
}
