use eonix_mm::address::{VAddr, VRange};

use crate::kernel::mem::address::VRangeExt;
use crate::kernel::mem::mm_list::mm_area::AreaList;
use crate::kernel::mem::mm_list::{AreaFlags, MemArea, MemListLock};
use crate::kernel::mem::{AnonMapping, MMList, Permission};

#[derive(Clone)]
pub struct ProgramBreak {
    start: VAddr,
    current: VAddr,
}

impl ProgramBreak {
    pub const fn new(start: VAddr) -> Self {
        Self {
            start,
            current: start,
        }
    }

    pub const fn null() -> Self {
        Self::new(VAddr::NULL)
    }

    pub fn get(&self) -> VAddr {
        self.current
    }

    pub fn is_null(&self) -> bool {
        self.current == VAddr::NULL
    }

    fn set(&mut self, pos: VAddr) {
        self.current = pos;
    }

    fn reset(&mut self, start: VAddr) {
        self.start = start;
        self.current = start;
    }
}

async fn set_break(
    areas: &mut AreaList, brk: &mut ProgramBreak, mm_lock: &mut MemListLock,
    newbrk: Option<VAddr>,
) -> VAddr {
    assert!(
        !brk.is_null(),
        "set_break called before program break is registered"
    );

    let curbrk = brk.get();
    let Some(newbrk) = newbrk else {
        // Reject NULL brks
        return curbrk;
    };

    if newbrk == curbrk {
        // Nothing to do
        return curbrk;
    }

    if newbrk < curbrk {
        // TODO: We should allow shrinking the break if there aren't non-brk
        //       mappings within the range. Reject now to keep things simple.
        return curbrk;
    }

    let new_range = VRange::new(curbrk, newbrk);
    if !new_range.is_user() || areas.contains_range(&new_range) {
        return curbrk;
    }

    let Some(area) = areas.upper_bound(curbrk) else {
        return expand_create_area(areas, brk, new_range);
    };

    let range = area.range.as_ref(&areas.lock);

    if range.end() != curbrk {
        // Someone might have unmapped the brk area.
        return expand_create_area(areas, brk, new_range);
    }

    let mut area_lock = area.lock.lock().await;
    let range = area.range.as_mut(mm_lock, &mut areas.lock, &mut area_lock);
    *range = range.grow(new_range.len());

    brk.set(newbrk);
    newbrk
}

fn expand_create_area(
    areas: &mut AreaList, brk: &mut ProgramBreak, new_range: VRange,
) -> VAddr {
    let area = MemArea::new(
        new_range,
        AreaFlags::from_old(
            Permission {
                read: true,
                write: true,
                execute: false,
            },
            false,
        ),
        AnonMapping::new(),
    );

    areas.insert(area);

    brk.set(new_range.end());
    new_range.end()
}

impl MMList {
    pub async fn set_break(&self, newbrk: Option<VAddr>) -> VAddr {
        let inner = self.inner.borrow();
        let mut inner = inner.lock().await;
        let inner = &mut *inner;

        set_break(
            &mut inner.areas,
            &mut inner.prog_break,
            &mut inner.lock,
            newbrk,
        )
        .await
    }

    /// This should be called only **once** for every thread.
    pub async fn register_break(&self, start: VAddr) {
        let inner = self.inner.borrow();
        let mut inner = inner.lock().await;
        let brk = &mut inner.prog_break;

        assert!(brk.is_null(), "program break already registered");
        brk.reset(start);
    }
}
