use crate::prelude::*;

use bindings::ENOENT;

pub struct Path<'lt> {
    all: &'lt [u8],
}

pub struct PathIterator<'lt> {
    rem: &'lt [u8],
}

impl<'lt> Path<'lt> {
    pub fn new(all: &'lt [u8]) -> KResult<Self> {
        if all.is_empty() {
            Err(ENOENT)
        } else {
            Ok(Self { all })
        }
    }

    pub fn from_str(all: &'lt str) -> KResult<Self> {
        Self::new(all.as_bytes())
    }

    pub fn is_absolute(&self) -> bool {
        self.all.starts_with(&['/' as u8])
    }

    pub fn iter(&self) -> PathIterator<'lt> {
        PathIterator::new(self.all)
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

impl<'lt> Iterator for PathIterator<'lt> {
    type Item = PathComponent<'lt>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rem.is_empty() {
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
            cur if cur.is_empty() => Some(PathComponent::TrailingEmpty),
            cur if cur == b"." => Some(PathComponent::Current),
            cur if cur == b".." => Some(PathComponent::Parent),
            cur => Some(PathComponent::Name(cur)),
        }
    }
}
