use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct ProcessPastelTxStatusReport<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    /// CHECK: Manual checks are performed in the instruction to ensure the contributor is valid and authorized. This includes verifying signatures and other relevant validations.
    #[account(mut)]
    pub contributor: Signer<'info>,
    // You can add other accounts as needed
}
