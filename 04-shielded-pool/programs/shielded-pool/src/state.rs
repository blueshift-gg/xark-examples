use pinocchio::{AccountView, Address, ProgramResult, error::ProgramError};

use crate::helpers::writable;

pub const TREE_HEIGHT: u32 = 20;
pub const ROOT_HISTORY_SIZE: usize = 16;

pub const POOL_SEED: &[u8] = b"pool";
pub const NULLIFIER_SEED: &[u8] = b"nullifier";

// The empty tree's root, little-endian: the first deposit's `old_root`. Fixed
// here so a pool can't be seeded with another; the client tests check it against
// the circuit's `Path.first().treeRoot()`, and the host tests against the deposit
// demo's `old_root`.
pub const EMPTY_ROOT: [u8; 32] = [
    26, 122, 47, 146, 164, 115, 191, 166, 88, 234, 28, 252, 38, 169, 221, 171, 246, 159, 204, 103,
    2, 229, 199, 103, 212, 104, 254, 41, 151, 59, 127, 46,
];

/// A pool's account, 536 bytes. Every field is a byte array, so the struct has
/// alignment 1 and every bit pattern is valid: the account's data is the struct.
///
/// | offset | size | field |
/// |---|---|---|
/// | 0 | 1 | tag, [`Pool::TAG`] |
/// | 1 | 1 | bump |
/// | 2 | 6 | reserved, zero |
/// | 8 | 8 | denomination (u64 LE) |
/// | 16 | 4 | next_index (u32 LE) |
/// | 20 | 4 | current_root_index (u32 LE) |
/// | 24 | 512 | roots: 16 × 32-byte little-endian field elements, a ring |
#[repr(C)]
pub struct Pool {
    pub tag: u8,
    pub bump: u8,
    reserved: [u8; 6],
    pub denomination: [u8; 8],
    next_index: [u8; 4],
    current_root_index: [u8; 4],
    roots: [[u8; 32]; ROOT_HISTORY_SIZE],
}

const _: () = assert!(Pool::LEN == 536 && align_of::<Pool>() == 1);

impl Pool {
    pub const LEN: usize = size_of::<Self>();
    pub const TAG: u8 = 1;

    /// The pool `data` holds, if it is a pool's size and tag.
    pub fn load(data: &[u8]) -> Result<&Self, ProgramError> {
        if data.len() != Self::LEN || data[0] != Self::TAG {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: `Pool` is plain bytes and `data` is exactly its size.
        Ok(unsafe { &*data.as_ptr().cast::<Self>() })
    }

    /// As [`Pool::load`], to write.
    pub fn load_mut(data: &mut [u8]) -> Result<&mut Self, ProgramError> {
        if data.len() != Self::LEN || data[0] != Self::TAG {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: as above.
        Ok(unsafe { &mut *data.as_mut_ptr().cast::<Self>() })
    }

    /// Write a new pool into `data`, a freshly created (zeroed) account: the rest
    /// of the ring is empty.
    pub fn init(data: &mut [u8], denomination: [u8; 8], bump: u8) -> ProgramResult {
        if data.len() != Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: as above.
        let pool = unsafe { &mut *data.as_mut_ptr().cast::<Self>() };
        pool.tag = Self::TAG;
        pool.bump = bump;
        pool.denomination = denomination;
        pool.roots[0] = EMPTY_ROOT;
        Ok(())
    }

    /// A pool account: writable, owned by this program, a pool's size and tag, at
    /// the address its own denomination and stored bump derive (`["pool",
    /// denomination]`). The bump is the canonical one `initialize` found, so this
    /// is one hash, not a search.
    pub fn check(account: &AccountView) -> ProgramResult {
        writable(account)?;
        if !account.owned_by(&crate::ID) {
            return Err(ProgramError::InvalidAccountOwner);
        }
        let data = account.try_borrow()?;
        let pool = Self::load(&data)?;
        let address = Address::derive_address(
            &[POOL_SEED, &pool.denomination],
            Some(pool.bump),
            &crate::ID,
        );
        if account.address() != &address {
            return Err(ProgramError::InvalidSeeds);
        }
        Ok(())
    }

    pub fn denomination(&self) -> u64 {
        u64::from_le_bytes(self.denomination)
    }

    pub fn next_index(&self) -> u32 {
        u32::from_le_bytes(self.next_index)
    }

    fn current_root_index(&self) -> usize {
        u32::from_le_bytes(self.current_root_index) as usize % ROOT_HISTORY_SIZE
    }

    pub fn current_root(&self) -> &[u8; 32] {
        &self.roots[self.current_root_index()]
    }

    /// Remember `root` as the newest entry in the ring and advance the tree. The
    /// caller has checked the tree isn't full, so the index doesn't overflow.
    pub fn push_root(&mut self, root: &[u8; 32]) {
        let next = (self.current_root_index() + 1) % ROOT_HISTORY_SIZE;
        self.current_root_index = (next as u32).to_le_bytes();
        self.roots[next] = *root;
        self.next_index = (self.next_index() + 1).to_le_bytes();
    }

    /// Is `root` one of the recent ring entries? (All-zero never matches: the
    /// unused ring slots are zeroed, and a real root is never zero.)
    pub fn knows_root(&self, root: &[u8; 32]) -> bool {
        root.iter().any(|&b| b != 0) && self.roots.iter().any(|r| r == root)
    }
}

/// A spent nullifier's marker account: one byte, its tag. Its existence is what
/// counts.
pub struct Nullifier;

impl Nullifier {
    pub const LEN: usize = 1;
    pub const TAG: u8 = 2;
}
