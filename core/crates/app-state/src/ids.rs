//! Id minting for `task-`, `ses-` and `box-` ids with reserved ranges.
//!
//! The number space of each kind is cut into [`BLOCK_COUNT`] blocks of
//! [`BLOCK_SIZE`] numbers. A device mints only from blocks it claimed in the
//! workspace document. Each block's next number is stored under the block
//! and read as the highest value any run wrote, so a later run continues
//! after every id an earlier run wrote to disk.
//!
//! - The device that creates the workspace claims block 0 (`task-1` to
//!   `task-999`) in its first change, and no other claim ever lands there.
//! - Every other claim tries blocks 1 to 9 999 in an order derived from a
//!   hash of the run's random actor, skipping blocks it sees claimed.
//! - A run that has not yet read every file of its own device folder may
//!   have missed ids an earlier run minted, so it mints only from blocks it
//!   claimed itself; once its own files have all arrived it may mint from
//!   any block its device holds.
//!
//! Two devices (or two runs) that both claim while neither has seen the
//! other's claim land on the same block with odds of 1 in 9 999. When the
//! claims meet, the smallest device id keeps the block and the other moves
//! to a new block before it mints again; ids it minted from the block before
//! then are the residual risk, documented in `SECURITY.md`.

use std::collections::BTreeSet;

use automerge::transaction::Transactable;
use automerge::{AutoCommit, ObjId};

use crate::device::DeviceId;
use crate::doc::{KEY_SEPARATOR, composite_key, get_all_str, get_all_u64, keys};
use crate::error::AppStateError;

/// Numbers per block.
pub const BLOCK_SIZE: u64 = 1_000;
/// Blocks per kind; ids stay below `BLOCK_COUNT * BLOCK_SIZE`.
pub const BLOCK_COUNT: u64 = 10_000;
/// How many hashed candidates a claim tries before it scans for a free block.
const HASHED_ATTEMPTS: u64 = 64;

/// Which id a device mints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IdKind {
    /// `task-` ids, written into the vault's task lines.
    Task,
    /// `ses-` ids of timer sessions.
    Session,
    /// `box-` ids of time boxes.
    TimeBox,
}

/// A block of ids a device claimed, as the Settings window shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdBlock {
    /// Which ids the block holds.
    pub kind: IdKind,
    /// The block's index.
    pub index: u64,
    /// The first number of the block.
    pub first: u64,
    /// The last number of the block.
    pub last: u64,
}

/// What id minting works on.
pub(crate) struct IdMaps<'a> {
    /// `kind/block` → the claiming device id.
    pub(crate) blocks: &'a ObjId,
    /// `kind/block/next` → the next free number, read as the highest value written.
    pub(crate) counters: &'a ObjId,
}

/// Who mints: the device, its run's actor bytes (the claim hash seed), and
/// which blocks this run claimed itself.
pub(crate) struct Minter<'a> {
    pub(crate) device: &'a DeviceId,
    pub(crate) run_seed: &'a [u8],
    pub(crate) run_blocks: &'a mut BTreeSet<(IdKind, u64)>,
    /// Whether every file of this device's folder has been read.
    pub(crate) has_whole_own_history: bool,
}

impl IdKind {
    /// The id prefix, such as `task-`.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            IdKind::Task => "task-",
            IdKind::Session => "ses-",
            IdKind::TimeBox => "box-",
        }
    }

    /// Every kind.
    pub const ALL: [IdKind; 3] = [IdKind::Task, IdKind::Session, IdKind::TimeBox];
}

impl IdBlock {
    fn new(kind: IdKind, index: u64) -> IdBlock {
        IdBlock {
            kind,
            index,
            first: block_first(index),
            last: index * BLOCK_SIZE + BLOCK_SIZE - 1,
        }
    }
}

/// Mints the next id of `kind`, skipping ids `is_taken` reports as already
/// in use (ids imported into the vault before the app).
pub(crate) fn mint(
    doc: &mut AutoCommit,
    maps: &IdMaps<'_>,
    minter: &mut Minter<'_>,
    kind: IdKind,
    is_taken: &dyn Fn(&str) -> bool,
) -> Result<String, AppStateError> {
    loop {
        let (block, next) = match usable_block(doc, maps, minter, kind) {
            Some(usable) => usable,
            None => {
                let block = claim_hashed_block(doc, maps, minter, kind)?;
                (block, block_first(block))
            }
        };
        doc.put(maps.counters, counter_key(kind, block), next + 1)?;
        let id = format!("{}{next}", kind.prefix());
        if !is_taken(&id) {
            return Ok(id);
        }
    }
}

/// Claims block 0 of every kind for the device creating the workspace.
pub(crate) fn claim_creator_blocks(
    doc: &mut AutoCommit,
    maps: &IdMaps<'_>,
    minter: &mut Minter<'_>,
) -> Result<(), AppStateError> {
    for kind in IdKind::ALL {
        claim(doc, maps, minter, kind, 0)?;
    }
    Ok(())
}

/// Every block `device` holds, by kind and index.
pub(crate) fn blocks_of(doc: &AutoCommit, maps: &IdMaps<'_>, device: &DeviceId) -> Vec<IdBlock> {
    let mut blocks: Vec<IdBlock> = held_blocks(doc, maps, device)
        .into_iter()
        .map(|(kind, index)| IdBlock::new(kind, index))
        .collect();
    blocks.sort_by_key(|block| (block.kind, block.index));
    blocks
}

/// The lowest block this run may mint from that has room, with its next number.
fn usable_block(
    doc: &AutoCommit,
    maps: &IdMaps<'_>,
    minter: &Minter<'_>,
    kind: IdKind,
) -> Option<(u64, u64)> {
    let mut candidates: Vec<u64> = held_blocks(doc, maps, minter.device)
        .into_iter()
        .filter(|(held_kind, index)| {
            *held_kind == kind
                && (minter.has_whole_own_history || minter.run_blocks.contains(&(kind, *index)))
        })
        .map(|(_, index)| index)
        .collect();
    candidates.sort_unstable();
    candidates.into_iter().find_map(|index| {
        let block = IdBlock::new(kind, index);
        let stored = get_all_u64(doc, maps.counters, &counter_key(kind, index))
            .into_iter()
            .max()
            .unwrap_or(0);
        let next = stored.max(block.first);
        (next <= block.last).then_some((index, next))
    })
}

fn claim_hashed_block(
    doc: &mut AutoCommit,
    maps: &IdMaps<'_>,
    minter: &mut Minter<'_>,
    kind: IdKind,
) -> Result<u64, AppStateError> {
    let block = (0..HASHED_ATTEMPTS)
        .map(|attempt| hashed_block(minter.run_seed, kind, attempt))
        .chain(1..BLOCK_COUNT)
        .find(|block| holder(doc, maps, kind, *block).is_none())
        .ok_or(AppStateError::IdSpaceExhausted(kind.prefix()))?;
    claim(doc, maps, minter, kind, block)?;
    Ok(block)
}

fn claim(
    doc: &mut AutoCommit,
    maps: &IdMaps<'_>,
    minter: &mut Minter<'_>,
    kind: IdKind,
    block: u64,
) -> Result<(), AppStateError> {
    doc.put(maps.blocks, block_key(kind, block), minter.device.as_str())?;
    minter.run_blocks.insert((kind, block));
    Ok(())
}

/// The blocks `device` holds, as `(kind, index)`.
fn held_blocks(doc: &AutoCommit, maps: &IdMaps<'_>, device: &DeviceId) -> Vec<(IdKind, u64)> {
    keys(doc, maps.blocks)
        .iter()
        .filter_map(|key| parse_block_key(key))
        .filter(|(kind, index)| holder(doc, maps, *kind, *index).as_ref() == Some(device))
        .collect()
}

/// Who holds a block: of concurrent claims, the smallest device id, whatever
/// Automerge picked as the winning value, so every device agrees.
fn holder(doc: &AutoCommit, maps: &IdMaps<'_>, kind: IdKind, block: u64) -> Option<DeviceId> {
    get_all_str(doc, maps.blocks, &block_key(kind, block))
        .into_iter()
        .filter_map(|id| DeviceId::new(id).ok())
        .min()
}

/// A block in 1..BLOCK_COUNT from a stable FNV-1a hash of seed, kind and attempt.
fn hashed_block(seed: &[u8], kind: IdKind, attempt: u64) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let bytes = seed
        .iter()
        .copied()
        .chain([0x1f])
        .chain(kind.prefix().bytes())
        .chain(attempt.to_le_bytes());
    let hash = bytes.fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    });
    1 + hash % (BLOCK_COUNT - 1)
}

/// Block 0 starts at 1: there is no `task-0`.
fn block_first(index: u64) -> u64 {
    (index * BLOCK_SIZE).max(1)
}

fn block_key(kind: IdKind, block: u64) -> String {
    composite_key(&[kind.prefix(), &block.to_string()])
}

/// Reads a claim key; indexes outside the block space are not claims.
fn parse_block_key(key: &str) -> Option<(IdKind, u64)> {
    let (prefix, index) = key.split_once(KEY_SEPARATOR)?;
    let kind = IdKind::ALL
        .into_iter()
        .find(|kind| kind.prefix() == prefix)?;
    let index: u64 = index.parse().ok()?;
    (index < BLOCK_COUNT).then_some((kind, index))
}

fn counter_key(kind: IdKind, block: u64) -> String {
    composite_key(&[kind.prefix(), &block.to_string(), "next"])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashed_blocks_are_stable_and_never_block_zero() {
        let seed = b"mac-a.0123456789abcdef";
        let first = hashed_block(seed, IdKind::Task, 0);
        assert_eq!(first, hashed_block(seed, IdKind::Task, 0));
        assert_ne!(first, hashed_block(seed, IdKind::Task, 1));
        for attempt in 0..10_000 {
            let block = hashed_block(seed, IdKind::Session, attempt);
            assert!((1..BLOCK_COUNT).contains(&block));
        }
    }

    #[test]
    fn claims_outside_the_block_space_are_ignored() {
        assert_eq!(
            parse_block_key(&block_key(IdKind::Task, 7)),
            Some((IdKind::Task, 7))
        );
        assert_eq!(parse_block_key(&block_key(IdKind::Task, BLOCK_COUNT)), None);
        assert_eq!(parse_block_key(&block_key(IdKind::Task, u64::MAX)), None);
    }

    #[test]
    fn block_bounds() {
        assert_eq!(IdBlock::new(IdKind::Task, 0).first, 1);
        assert_eq!(IdBlock::new(IdKind::Task, 0).last, 999);
        assert_eq!(IdBlock::new(IdKind::Task, 42).first, 42_000);
        assert_eq!(IdBlock::new(IdKind::Task, 42).last, 42_999);
    }
}
