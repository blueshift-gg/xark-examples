use pinocchio::{
    AccountView, Address,
    account::{Ref, RefMut},
    error::ProgramError,
};

pub const DISTRIBUTOR_SEED: &[u8] = b"distributor";
pub const VAULT_SEED: &[u8] = b"vault";
pub const CLAIMED_SEED: &[u8] = b"claimed";

/// An SPL token account's size.
pub const TOKEN_ACCOUNT_LEN: usize = 165;

/// A list's distributor, at `["distributor", root]`. Its vault is at
/// `["vault", distributor]`, a token account the distributor owns. Every field is
/// bytes, so the layout has no padding and any address is aligned.
#[repr(C)]
pub struct Distributor {
    pub root: [u8; 32],
    pub mint: Address,
    pub bump: u8,
    pub vault_bump: u8,
}

impl Distributor {
    pub const LEN: usize = size_of::<Self>();

    /// The distributor in `account`. Only `initialize` makes an account of the
    /// program's this size (a receipt is the program's too, but empty).
    pub fn load(account: &AccountView) -> Result<Ref<'_, Self>, ProgramError> {
        if !account.owned_by(&crate::ID) || account.data_len() != Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: the data is `LEN` bytes, and `Self` is bytes only (alignment 1).
        Ok(Ref::map(account.try_borrow()?, |data| unsafe {
            &*(data.as_ptr() as *const Self)
        }))
    }

    pub fn load_mut(account: &mut AccountView) -> Result<RefMut<'_, Self>, ProgramError> {
        if !account.owned_by(&crate::ID) || account.data_len() != Self::LEN {
            return Err(ProgramError::InvalidAccountData);
        }
        // SAFETY: as in `load`.
        Ok(RefMut::map(account.try_borrow_mut()?, |data| unsafe {
            &mut *(data.as_mut_ptr() as *mut Self)
        }))
    }
}
