use crate::{error::*, state::*};
use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

#[derive(Accounts)]
pub struct WithdrawFunds<'info> {
    #[account(
        mut,
        constraint = oracle_contract_state.admin_pubkey == *admin_account.key @ OracleError::UnauthorizedWithdrawalAccount,
    )]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    /// CHECK: The admin_account is manually verified in the instruction to ensure it's the correct and authorized account for withdrawal operations. This includes checking if the account matches the admin_pubkey stored in oracle_contract_state.
    pub admin_account: AccountInfo<'info>,

    /// CHECK: OK
    #[account(mut, seeds = [b"reward_pool"], bump)]
    pub reward_pool_account: UncheckedAccount<'info>,
    /// CHECK: OK
    #[account(mut, seeds = [b"fee_receiving_contract"], bump)]
    pub fee_receiving_contract_account: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

impl<'info> WithdrawFunds<'info> {
    pub fn execute(
        ctx: Context<WithdrawFunds>,
        reward_pool_amount: u64,
        fee_receiving_amount: u64,
    ) -> Result<()> {
        if !ctx.accounts.admin_account.is_signer {
            return Err(OracleError::UnauthorizedWithdrawalAccount.into()); // Check if the admin_account is a signer
        }
        let admin_account = &mut ctx.accounts.admin_account;
        let reward_pool_account = &mut ctx.accounts.reward_pool_account;
        let fee_receiving_contract_account = &mut ctx.accounts.fee_receiving_contract_account;

        // Transfer SOL from the reward pool account to the admin account
        if **reward_pool_account.to_account_info().lamports.borrow() < reward_pool_amount {
            return Err(OracleError::InsufficientFunds.into());
        }
        transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.reward_pool_account.to_account_info(),
                    to: admin_account.to_account_info(),
                },
            )
            .with_signer(&[&[b"reward_pool", &[ctx.bumps.reward_pool_account]]]),
            reward_pool_amount,
        )?;

        // Transfer SOL from the fee receiving contract account to the admin account
        if **fee_receiving_contract_account
            .to_account_info()
            .lamports
            .borrow()
            < fee_receiving_amount
        {
            return Err(OracleError::InsufficientFunds.into());
        }
        transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: fee_receiving_contract_account.to_account_info(),
                    to: admin_account.to_account_info(),
                },
            )
            .with_signer(&[&[
                b"fee_receiving_contract",
                &[ctx.bumps.fee_receiving_contract_account],
            ]]),
            fee_receiving_amount,
        )?;

        msg!("Withdrawal successful: {} lamports transferred from reward pool and {} lamports from fee receiving contract to admin account", reward_pool_amount, fee_receiving_amount);
        Ok(())
    }
}
