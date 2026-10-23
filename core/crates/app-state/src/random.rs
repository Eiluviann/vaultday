//! Random bytes from the operating system, for keys, nonces and device ids.

use crate::error::AppStateError;

/// `N` random bytes.
pub(crate) fn bytes<const N: usize>() -> Result<[u8; N], AppStateError> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(AppStateError::Randomness)?;
    Ok(bytes)
}

/// Fills `buffer` with random bytes; for key material the caller owns in a zeroizing buffer.
pub(crate) fn fill(buffer: &mut [u8]) -> Result<(), AppStateError> {
    getrandom::fill(buffer).map_err(AppStateError::Randomness)
}
