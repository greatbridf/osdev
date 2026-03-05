use alloc::sync::Arc;

use super::constants::EEXIST;
use super::terminal::Terminal;
use crate::prelude::*;

static CONSOLE: Spin<Option<Arc<Terminal>>> = Spin::new(None);

pub fn set_console(terminal: Arc<Terminal>) -> KResult<()> {
    let mut console = CONSOLE.lock();
    if console.is_none() {
        *console = Some(terminal);
        Ok(())
    } else {
        Err(EEXIST)
    }
}

pub fn get_console() -> Option<Arc<Terminal>> {
    let console = CONSOLE.lock();
    console.clone()
}
