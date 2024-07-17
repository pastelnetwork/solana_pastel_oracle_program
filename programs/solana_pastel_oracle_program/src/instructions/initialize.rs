use crate::{error::*, state::*};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = user, space = 10_240)] // Adjusted space
    pub oracle_contract_state: Account<'info, OracleContractState>,
    #[account(mut)]
    pub user: Signer<'info>,

    // Account for TempTxStatusReportAccount PDA
    #[account(
        init,
        seeds = [b"temp_tx_status_report"],
        bump,
        payer = user,
        space = 10_240
    )]
    pub temp_report_account: Account<'info, TempTxStatusReportAccount>,

    // Account for ContributorDataAccount PDA
    #[account(
        init,
        seeds = [b"contributor_data"],
        bump,
        payer = user,
        space = 10_240
    )]
    pub contributor_data_account: Account<'info, ContributorDataAccount>,

    // Account for TxidSubmissionCountsAccount PDA
    #[account(
        init,
        seeds = [b"txid_submission_counts"],
        bump,
        payer = user,
        space = 10_240
    )]
    pub txid_submission_counts_account: Account<'info, TxidSubmissionCountsAccount>,

    // Account for AggregatedConsensusDataAccount PDA
    #[account(
        init,
        seeds = [b"aggregated_consensus_data"],
        bump,
        payer = user,
        space = 10_240
    )]
    pub aggregated_consensus_data_account: Account<'info, AggregatedConsensusDataAccount>,

    // System program is needed for account creation
    pub system_program: Program<'info, System>,
}

impl<'info> Initialize<'info> {
    pub fn initialize_oracle_state(&mut self) -> Result<()> {
        msg!("Setting up Oracle Contract State");

        let state = &mut self.oracle_contract_state;
        // Ensure the oracle_contract_state is not already initialized
        if state.is_initialized {
            return Err(OracleError::AccountAlreadyInitialized.into());
        }

        state.is_initialized = true;
        state.admin_pubkey = self.user.key();
        msg!("Admin Pubkey set to: {:?}", self.user.key());

        state.monitored_txids = Vec::new();
        msg!("Monitored Txids Vector initialized");

        state.bridge_contract_pubkey = Pubkey::default();
        msg!("Bridge Contract Pubkey set to default");

        msg!("Oracle Contract State Initialization Complete");
        Ok(())
    }
}
