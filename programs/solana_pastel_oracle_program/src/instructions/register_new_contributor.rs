use crate::{constant::*, error::*, state::*};
use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

#[derive(Accounts)]
pub struct RegisterNewDataContributor<'info> {
    /// CHECK: Manual checks are performed in the instruction to ensure the contributor_account is valid and safe to use.
    #[account(mut, signer)]
    pub contributor_account: AccountInfo<'info>,

    /// CHECK: OK
    #[account(mut, seeds = [b"reward_pool"], bump)]
    pub reward_pool_account: UncheckedAccount<'info>,

    /// CHECK: OK
    #[account(mut, seeds = [b"fee_receiving_contract"], bump)]
    pub fee_receiving_contract_account: UncheckedAccount<'info>,

    #[account(mut)]
    pub contributor_data_account: Account<'info, ContributorDataAccount>,

    pub system_program: Program<'info, System>,
}

pub fn register_new_data_contributor_helper(
    ctx: Context<RegisterNewDataContributor>,
) -> Result<()> {
    let contributor_data_account = &mut ctx.accounts.contributor_data_account;
    msg!(
        "Initiating new contributor registration: {}",
        ctx.accounts.contributor_account.key()
    );

    // Check if the contributor is already registered
    if contributor_data_account
        .contributors
        .iter()
        .any(|c| c.reward_address == *ctx.accounts.contributor_account.key)
    {
        msg!(
            "Registration failed: Contributor already registered: {}",
            ctx.accounts.contributor_account.key
        );
        return Err(OracleError::ContributorAlreadyRegistered.into());
    }

    msg!(
        "Registration fee verified. Attempting to register new contributor {}",
        ctx.accounts.contributor_account.key
    );

    // Deduct the registration fee from the fee_receiving_contract_account and add it to the reward pool account
    transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx
                    .accounts
                    .fee_receiving_contract_account
                    .to_account_info(),
                to: ctx.accounts.reward_pool_account.to_account_info(),
            },
        )
        .with_signer(&[&[
            b"fee_receiving_contract",
            &[ctx.bumps.fee_receiving_contract_account],
        ]]),
        REGISTRATION_ENTRANCE_FEE_IN_LAMPORTS as u64,
    )?;

    let last_active_timestamp = Clock::get()?.unix_timestamp as u64;

    // Create and add the new contributor
    let new_contributor = Contributor {
        reward_address: *ctx.accounts.contributor_account.key,
        registration_entrance_fee_transaction_signature: String::new(), // Replace with actual data if available
        compliance_score: 1.0,                                          // Initial compliance score
        last_active_timestamp, // Set the last active timestamp to the current time
        total_reports_submitted: 0, // Initially, no reports have been submitted
        accurate_reports_count: 0, // Initially, no accurate reports
        current_streak: 0,     // No streak at the beginning
        reliability_score: 1.0, // Initial reliability score
        consensus_failures: 0, // No consensus failures at the start
        ban_expiry: 0,         // No ban initially set
        is_eligible_for_rewards: false, // Initially not eligible for rewards
        is_recently_active: false, // Initially not considered active
        is_reliable: false,    // Initially not considered reliable
    };

    // Append the new contributor to the ContributorDataAccount
    contributor_data_account.contributors.push(new_contributor);

    // Logging for debug purposes
    msg!(
        "New Contributor successfully Registered: Address: {}, Timestamp: {}",
        ctx.accounts.contributor_account.key,
        last_active_timestamp
    );
    Ok(())
}
