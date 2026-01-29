use core::fmt::{self, Debug, Formatter};

use crate::kernel::constants::ENOENT;
use crate::prelude::*;

#[repr(transparent)]
pub struct Path {
    all: [u8],
}

pub struct PathIterator<'lt> {
    rem: &'lt [u8],
}

impl Path {
    pub fn new(all: &[u8]) -> KResult<&Self> {
        if all.is_empty() {
            Err(ENOENT)
        } else {
            Ok(unsafe { &*(all as *const [u8] as *const Path) })
        }
    }

    pub fn is_absolute(&self) -> bool {
        self.all.starts_with(&['/' as u8])
    }

    pub fn iter(&self) -> PathIterator<'_> {
        PathIterator::new(&self.all)
    }
}

impl<'lt> PathIterator<'lt> {
    fn new(all: &'lt [u8]) -> Self {
        Self { rem: all }
    }
}

#[derive(Debug)]
pub enum PathComponent<'lt> {
    Name(&'lt [u8]),
    TrailingEmpty,
    Current,
    Parent,
}

impl PathIterator<'_> {
    pub fn is_empty(&self) -> bool {
        self.rem.is_empty()
    }
}

impl<'lt> Iterator for PathIterator<'lt> {
    type Item = PathComponent<'lt>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_empty() {
            return None;
        }

        let trimmed = self
            .rem
            .iter()
            .position(|&c| c != '/' as u8)
            .map(|pos| self.rem.split_at(pos).1)
            .unwrap_or(&[]);

        let next_start = trimmed
            .iter()
            .position(|&c| c == '/' as u8)
            .unwrap_or(trimmed.len());

        let (cur, rem) = trimmed.split_at(next_start);

        self.rem = rem;

        match cur {
            b"" => Some(PathComponent::TrailingEmpty),
            b"." => Some(PathComponent::Current),
            b".." => Some(PathComponent::Parent),
            name => Some(PathComponent::Name(name)),
        }
    }
}

impl Debug for Path {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Path({:?})", &self.all)
    }
}
