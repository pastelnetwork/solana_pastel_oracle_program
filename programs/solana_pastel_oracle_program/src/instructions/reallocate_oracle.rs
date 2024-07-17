use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct ReallocateOracleState<'info> {
    #[account(mut, has_one = admin_pubkey)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    pub admin_pubkey: Signer<'info>,
    pub system_program: Program<'info, System>,
    #[account(mut)]
    pub temp_report_account: Account<'info, TempTxStatusReportAccount>,
    #[account(mut)]
    pub contributor_data_account: Account<'info, ContributorDataAccount>,
    #[account(mut)]
    pub txid_submission_counts_account: Account<'info, TxidSubmissionCountsAccount>,
    #[account(mut)]
    pub aggregated_consensus_data_account: Account<'info, AggregatedConsensusDataAccount>,
}

impl<'info> ReallocateOracleState<'info> {
    pub fn execute(ctx: Context<ReallocateOracleState>) -> Result<()> {
        let oracle_contract_state = &mut ctx.accounts.oracle_contract_state;

        // Calculate new size; add 10,240 bytes for each reallocation
        // Ensure not to exceed 100KB total size
        let current_size = oracle_contract_state.to_account_info().data_len();
        let additional_space = 10_240; // Increment size
        let max_size = 100 * 1024; // 100KB
        let new_size = std::cmp::min(current_size + additional_space, max_size);

        // Perform reallocation
        oracle_contract_state
            .to_account_info()
            .realloc(new_size, false)?;

        msg!("OracleContractState reallocated to new size: {}", new_size);

        reallocate_temp_report_account(&mut ctx.accounts.temp_report_account)?;
        reallocate_contributor_data_account(&mut ctx.accounts.contributor_data_account)?;
        reallocate_submission_counts_account(&mut ctx.accounts.txid_submission_counts_account)?;
        reallocate_aggregated_consensus_data_account(
            &mut ctx.accounts.aggregated_consensus_data_account,
        )?;
        Ok(())
    }
}

pub fn reallocate_temp_report_account(
    temp_report_account: &mut Account<'_, TempTxStatusReportAccount>,
) -> Result<()> {
    // Define the threshold at which to reallocate (e.g., 90% full)
    const REALLOCATION_THRESHOLD: f32 = 0.9;
    const ADDITIONAL_SPACE: usize = 10_240;
    const MAX_SIZE: usize = 100 * 1024;

    let current_size = temp_report_account.to_account_info().data_len();
    let current_usage =
        temp_report_account.reports.len() * std::mem::size_of::<TempTxStatusReport>();
    let usage_ratio = current_usage as f32 / current_size as f32;

    if usage_ratio > REALLOCATION_THRESHOLD {
        let new_size = std::cmp::min(current_size + ADDITIONAL_SPACE, MAX_SIZE);
        temp_report_account
            .to_account_info()
            .realloc(new_size, false)?;
        msg!(
            "TempTxStatusReportAccount reallocated to new size: {}",
            new_size
        );
    }

    Ok(())
}

pub fn reallocate_contributor_data_account(
    contributor_data_account: &mut Account<'_, ContributorDataAccount>,
) -> Result<()> {
    // Define the threshold at which to reallocate (e.g., 90% full)
    const REALLOCATION_THRESHOLD: f32 = 0.9;
    const ADDITIONAL_SPACE: usize = 10_240;
    const MAX_SIZE: usize = 100 * 1024;

    let current_size = contributor_data_account.to_account_info().data_len();
    let current_usage =
        contributor_data_account.contributors.len() * std::mem::size_of::<Contributor>();
    let usage_ratio = current_usage as f32 / current_size as f32;

    if usage_ratio > REALLOCATION_THRESHOLD {
        let new_size = std::cmp::min(current_size + ADDITIONAL_SPACE, MAX_SIZE);
        contributor_data_account
            .to_account_info()
            .realloc(new_size, false)?;
        msg!(
            "ContributorDataAccount reallocated to new size: {}",
            new_size
        );
    }

    Ok(())
}

pub fn reallocate_submission_counts_account(
    submission_counts_account: &mut Account<'_, TxidSubmissionCountsAccount>,
) -> Result<()> {
    // Define the threshold at which to reallocate (e.g., 90% full)
    const REALLOCATION_THRESHOLD: f32 = 0.9;
    const ADDITIONAL_SPACE: usize = 10_240;
    const MAX_SIZE: usize = 100 * 1024;

    let current_size = submission_counts_account.to_account_info().data_len();
    let current_usage = submission_counts_account.submission_counts.len()
        * std::mem::size_of::<TxidSubmissionCount>();
    let usage_ratio = current_usage as f32 / current_size as f32;

    if usage_ratio > REALLOCATION_THRESHOLD {
        let new_size = std::cmp::min(current_size + ADDITIONAL_SPACE, MAX_SIZE);
        submission_counts_account
            .to_account_info()
            .realloc(new_size, false)?;
        msg!(
            "TxidSubmissionCountsAccount reallocated to new size: {}",
            new_size
        );
    }

    Ok(())
}

pub fn reallocate_aggregated_consensus_data_account(
    aggregated_consensus_data_account: &mut Account<'_, AggregatedConsensusDataAccount>,
) -> Result<()> {
    // Define the threshold at which to reallocate (e.g., 90% full)
    const REALLOCATION_THRESHOLD: f32 = 0.9;
    const ADDITIONAL_SPACE: usize = 10_240;
    const MAX_SIZE: usize = 100 * 1024;

    let current_size = aggregated_consensus_data_account
        .to_account_info()
        .data_len();
    let current_usage = aggregated_consensus_data_account.consensus_data.len()
        * std::mem::size_of::<AggregatedConsensusData>();
    let usage_ratio = current_usage as f32 / current_size as f32;

    if usage_ratio > REALLOCATION_THRESHOLD {
        let new_size = std::cmp::min(current_size + ADDITIONAL_SPACE, MAX_SIZE);
        aggregated_consensus_data_account
            .to_account_info()
            .realloc(new_size, false)?;
        msg!(
            "AggregatedConsensusDataAccount reallocated to new size: {}",
            new_size
        );
    }

    Ok(())
}
