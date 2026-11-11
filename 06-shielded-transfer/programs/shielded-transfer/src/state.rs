use pinocchio::{
    AccountView, ProgramResult,
    account::{Ref, RefMut},
    error::ProgramError,
};

pub const TREE_HEIGHT: u32 = 20;
pub const ROOT_HISTORY_SIZE: usize = 16;

pub const POOL_SEED: &[u8] = b"pool";
pub const VAULT_SEED: &[u8] = b"vault";
pub const NULLIFIER_SEED: &[u8] = b"nullifier";

/// The empty tree's root, little-endian: the first transaction's `old_root`. The
/// client tests check it against the circuit's `Path.first().treeRoot()`.
pub const EMPTY_ROOT: [u8; 32] = [
    26, 122, 47, 146, 164, 115, 191, 166, 88, 234, 28, 252, 38, 169, 221, 171, 246, 159, 204, 103,
    2, 229, 199, 103, 212, 104, 254, 41, 151, 59, 127, 46,
];

/// A spent nullifier is a program-owned account at its PDA holding no data: its
/// existence is the mark, and the rent-exempt minimum for zero bytes is its cost.
pub const NULLIFIER_LEN: usize = 0;

/// The pool's account data, 554 bytes, every field a byte array so the layout has
/// no padding and any alignment: `mint` at 0, `next_index` (u32 LE) at 32,
/// `current_root_index` (u32 LE) at 36, the ring of `roots` at 40, `bump` at 552,
/// `vault_bump` at 553.
#[repr(C)]
pub struct Pool {
    pub mint: [u8; 32],
    pub next_index: [u8; 4],
    pub current_root_index: [u8; 4],
    pub roots: [[u8; 32]; ROOT_HISTORY_SIZE],
    pub bump: u8,
    pub vault_bump: u8,
}

const _: () = assert!(core::mem::align_of::<Pool>() == 1);

impl Pool {
    pub const LEN: usize = core::mem::size_of::<Pool>();

    /// The pool `account` holds: this program's, of a pool's size.
    pub fn from_account_view(account: &AccountView) -> Result<Ref<'_, Pool>, ProgramError> {
        if !account.owned_by(&crate::ID) {
            return Err(ProgramError::InvalidAccountOwner);
        }
        Ref::try_map(account.try_borrow()?, Self::load).map_err(|(_, e)| e)
    }

    /// As `from_account_view`, to write.
    pub fn from_account_view_mut(
        account: &mut AccountView,
    ) -> Result<RefMut<'_, Pool>, ProgramError> {
        if !account.owned_by(&crate::ID) {
            return Err(ProgramError::InvalidAccountOwner);
        }
        RefMut::try_map(account.try_borrow_mut()?, Self::load_mut).map_err(|(_, e)| e)
    }

    pub fn load(data: &[u8]) -> Result<&Pool, ProgramError> {
        if data.len() != Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: `Pool` is `repr(C)`, of byte arrays only (alignment 1, no
        // padding, every bit pattern valid), and `data` is exactly its size.
        Ok(unsafe { &*(data.as_ptr() as *const Pool) })
    }

    pub fn load_mut(data: &mut [u8]) -> Result<&mut Pool, ProgramError> {
        if data.len() != Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: as in `load`, and the borrow is unique.
        Ok(unsafe { &mut *(data.as_mut_ptr() as *mut Pool) })
    }

    pub fn current_root(&self) -> &[u8; 32] {
        &self.roots[u32::from_le_bytes(self.current_root_index) as usize % ROOT_HISTORY_SIZE]
    }

    /// Remember `root` as the newest ring entry; the tree grew by one pair.
    pub fn push_root(&mut self, root: &[u8; 32]) -> ProgramResult {
        let index = (u32::from_le_bytes(self.current_root_index) + 1) % ROOT_HISTORY_SIZE as u32;
        self.current_root_index = index.to_le_bytes();
        self.roots[index as usize] = *root;
        let next = u32::from_le_bytes(self.next_index)
            .checked_add(2)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        self.next_index = next.to_le_bytes();
        Ok(())
    }

    /// Is `root` one of the recent ring entries? (All-zero never matches.)
    pub fn knows_root(&self, root: &[u8; 32]) -> bool {
        root.iter().any(|&b| b != 0) && self.roots.iter().any(|r| r == root)
    }
}
