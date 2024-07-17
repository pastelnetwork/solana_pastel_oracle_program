use crate::{error::*, helper::create_seed, state::*};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(txid: String)]
pub struct HandlePendingPayment<'info> {
    #[account(
        init_if_needed,
        payer = user,
        seeds = [create_seed("pending_payment", &txid, &user.key()).as_ref()],
        bump,
        space = 8 + std::mem::size_of::<PendingPayment>() + 64 // Adjusted for discriminator
    )]
    pub pending_payment_account: Account<'info, PendingPaymentAccount>,

    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn add_pending_payment_helper(
    ctx: Context<HandlePendingPayment>,
    txid: String,
    pending_payment: PendingPayment,
) -> Result<()> {
    let pending_payment_account = &mut ctx.accounts.pending_payment_account;

    // Ensure the account is being initialized for the first time to avoid re-initialization
    if !pending_payment_account.pending_payment.txid.is_empty()
        && pending_payment_account.pending_payment.txid != txid
    {
        return Err(OracleError::PendingPaymentAlreadyInitialized.into());
    }

    // Ensure txid is correct and other fields are properly set
    if pending_payment.txid != txid {
        return Err(OracleError::InvalidTxid.into());
    }

    // Store the pending payment in the account
    pending_payment_account.pending_payment = pending_payment;

    msg!(
        "Pending payment account initialized: TXID: {}, Expected Amount: {}, Status: {:?}",
        pending_payment_account.pending_payment.txid,
        pending_payment_account.pending_payment.expected_amount,
        pending_payment_account.pending_payment.payment_status
    );

    Ok(())
}
