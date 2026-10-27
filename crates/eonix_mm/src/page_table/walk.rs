use super::pte::{RawAttribute, TableAttribute};
use super::{PageTableLevel, PTE};
use crate::address::{AddrOps, VAddr, VRange};
use crate::paging::PFN;

pub enum WalkState {
    Next,
    Skip,
    Break,
}

pub trait PageTable: Sized {
    type Entry: PTE;
    const LEVELS: &'static [PageTableLevel];

    fn index(&self, index: usize) -> &Self::Entry;
    fn index_mut(&mut self, index: usize) -> &mut Self::Entry;

    fn from_pfn(pfn: PFN) -> Self;
    unsafe fn take_pfn(pfn: PFN) -> Self;
}

pub struct PageTableWalk<'a, T, D>
where
    T: PageTable,
{
    levels: &'a [PageTableLevel],
    fill_entry: &'a [fn(&mut D, &mut T::Entry) -> Option<PFN>],
    walk_entry: &'a [fn(&mut D, &mut T::Entry) -> WalkState],
    data: D,
}

fn try_get_table<T, D>(
    entry: &mut T::Entry,
    data: &mut D,
    fill_entry: fn(&mut D, &mut T::Entry) -> Option<PFN>,
) -> Option<T>
where
    T: PageTable,
{
    let (mut pfn, attr) = entry.get();

    // Always skip huge page entries
    let attr = attr.as_table_attr()?;

    // For normal entries, check present flags
    if !attr.contains(TableAttribute::PRESENT) {
        // Skip entries filled with nothing
        pfn = fill_entry(data, entry)?;
    }

    Some(T::from_pfn(pfn))
}

fn _walk_page_table<T, D>(
    walk: &mut PageTableWalk<T, D>,
    cur_level: usize,
    table: &mut T,
    range: VRange,
) where
    T: PageTable,
{
    let level = walk.levels[cur_level];

    let page_size = level.page_size();
    let mut addr = range.start();

    while addr < range.end() {
        let idx = level.index_of(addr);
        let entry = table.index_mut(idx);

        let mut next_table = None;
        if cur_level < walk.levels.len() - 1 {
            next_table = try_get_table(entry, &mut walk.data, walk.fill_entry[cur_level]);
        }

        match (
            walk.walk_entry[cur_level](&mut walk.data, entry),
            &mut next_table,
        ) {
            (WalkState::Break, _) => break,
            (WalkState::Next, Some(next_table)) => _walk_page_table(
                walk,
                cur_level + 1,
                next_table,
                VRange::new(addr, range.end()),
            ),
            // `fill_entry` says that we shouldn't continue.
            (WalkState::Next, None) => {}
            _ => {}
        }

        addr = addr.floor_to(page_size) + page_size;
    }
}

pub fn walk_page_table<T, D>(walk: &mut PageTableWalk<T, D>, table: &mut T, range: VRange)
where
    T: PageTable,
{
    _walk_page_table(walk, 0, table, range);
}

pub fn drop_user_page_table<T>(mut root_page_table: T)
where
    T: PageTable,
{
    fn walk<T: PageTable, const LEVEL: usize>(_: &mut (), entry: &mut T::Entry) -> WalkState {
        let (pfn, attr) = entry.get();
        let Some(attr) = attr.as_table_attr() else {
            return WalkState::Skip;
        };

        if !attr.contains(TableAttribute::USER) {
            return WalkState::Skip;
        }

        unsafe {
            // Check `_walk_page_table`: We will and only will touch the next level of table with
            // `next_table` holding a refcount. We take the table away from the parent table now.
            T::take_pfn(pfn);
        }

        entry.set(PFN::from_val(0), TableAttribute::empty().into());

        if LEVEL == 2 {
            WalkState::Skip
        } else {
            WalkState::Next
        }
    }

    let mut walk = PageTableWalk {
        levels: T::LEVELS,
        fill_entry: &[no_fill::<T, ()>, no_fill::<T, ()>, no_fill::<T, ()>],
        walk_entry: &[walk::<T, 0>, walk::<T, 1>, walk::<T, 2>, skip_walk::<T, ()>],
        data: (),
    };

    walk_page_table(
        &mut walk,
        &mut root_page_table,
        VRange::new(VAddr::from(0), VAddr::from(0x0000_8000_0000_0000)),
    );
}

pub fn iter_pte<T: PageTable>(
    page_table: &mut T,
    range: VRange,
    fill_func: impl FnMut(&mut T::Entry) -> Option<PFN>,
    for_each: impl FnMut(&mut T::Entry),
) {
    let walker = (fill_func, for_each);

    fn fill_entry<T: PageTable>(
        (fill, _): &mut (
            impl FnMut(&mut T::Entry) -> Option<PFN>,
            impl FnMut(&mut T::Entry),
        ),
        entry: &mut T::Entry,
    ) -> Option<PFN> {
        fill(entry)
    }

    fn walk_entry<T: PageTable>(
        (_, for_each): &mut (
            impl FnMut(&mut T::Entry) -> Option<PFN>,
            impl FnMut(&mut T::Entry),
        ),
        entry: &mut T::Entry,
    ) -> WalkState {
        for_each(entry);
        WalkState::Next
    }

    let mut walk = PageTableWalk {
        levels: T::LEVELS,
        fill_entry: &[fill_entry::<T>, fill_entry::<T>, fill_entry::<T>],
        walk_entry: &[
            cont_walk::<T, _>,
            cont_walk::<T, _>,
            cont_walk::<T, _>,
            walk_entry::<T>,
        ],
        data: walker,
    };

    walk_page_table(&mut walk, page_table, range);
}

pub fn no_fill<T, D>(_: &mut D, _: &mut T::Entry) -> Option<PFN>
where
    T: PageTable,
{
    None
}

pub fn skip_walk<T, D>(_: &mut D, _: &mut T::Entry) -> WalkState
where
    T: PageTable,
{
    WalkState::Skip
}

pub fn cont_walk<T, D>(_: &mut D, _: &mut T::Entry) -> WalkState
where
    T: PageTable,
{
    WalkState::Next
}
