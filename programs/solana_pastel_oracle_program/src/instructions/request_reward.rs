use crate::{constant::BASE_REWARD_AMOUNT_IN_LAMPORTS, error::*, state::*};
use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

#[derive(Accounts)]
pub struct RequestReward<'info> {
    /// CHECK: OK
    #[account(mut, seeds = [b"reward_pool"], bump)]
    pub reward_pool_account: UncheckedAccount<'info>,
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    #[account(mut)]
    pub contributor_data_account: Account<'info, ContributorDataAccount>,
    /// CHECK: This is the account we're transferring lamports to
    #[account(mut)]
    pub contributor: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

pub fn request_reward_helper(
    ctx: Context<RequestReward>,
    contributor_address: Pubkey,
) -> Result<()> {
    let contributor_data_account = &ctx.accounts.contributor_data_account;
    let reward_pool_account = &ctx.accounts.reward_pool_account;
    let contributor_account = &ctx.accounts.contributor;

    // Find the contributor in the PDA and check eligibility
    let contributor = contributor_data_account
        .contributors
        .iter()
        .find(|c| c.reward_address == contributor_address)
        .ok_or(OracleError::UnregisteredOracle)?;

    let current_unix_timestamp = Clock::get()?.unix_timestamp as u64;

    if !contributor.is_eligible_for_rewards {
        msg!(
            "Contributor is not eligible for rewards: {}",
            contributor_address
        );
        return Err(OracleError::NotEligibleForReward.into());
    }

    if contributor.calculate_is_banned(current_unix_timestamp) {
        msg!("Contributor is banned: {}", contributor_address);
        return Err(OracleError::ContributorBanned.into());
    }

    let reward_amount = BASE_REWARD_AMOUNT_IN_LAMPORTS;

    // Ensure the reward pool has sufficient funds
    if reward_pool_account.to_account_info().lamports() < reward_amount {
        msg!("Insufficient funds in reward pool");
        return Err(OracleError::InsufficientFunds.into());
    }

    // Transfer the reward from the reward pool to the contributor
    transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.reward_pool_account.to_account_info(),
                to: contributor_account.to_account_info(),
            },
        )
        .with_signer(&[&[b"reward_pool", &[ctx.bumps.reward_pool_account]]]),
        reward_amount,
    )?;

    msg!(
        "Paid out Valid Reward Request: Contributor: {}, Amount: {}",
        contributor_address,
        reward_amount
    );

    Ok(())
}
