use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(txid: String)]
pub struct HandleConsensus<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,
}
