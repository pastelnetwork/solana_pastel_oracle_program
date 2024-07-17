use crate::{
    constant::{COST_IN_LAMPORTS_OF_ADDING_PASTEL_TXID_FOR_MONITORING, MAX_TXID_LENGTH},
    enumeration::PaymentStatus,
    error::*,
    state::*,
};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct AddTxidForMonitoringData {
    pub txid: String,
}

#[derive(Accounts)]
pub struct AddTxidForMonitoring<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    /// CHECK: The caller is manually verified in the instruction logic to ensure it's the correct and authorized account.
    #[account(signer)]
    pub caller: AccountInfo<'info>,

    // The `pending_payment_account` will be initialized in the function
    #[account(mut)]
    pub pending_payment_account: Account<'info, PendingPaymentAccount>,

    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

pub fn add_txid_for_monitoring_helper(
    ctx: Context<AddTxidForMonitoring>,
    data: AddTxidForMonitoringData,
) -> Result<()> {
    let state = &mut ctx.accounts.oracle_contract_state;

    if ctx.accounts.caller.key != &state.bridge_contract_pubkey {
        return Err(OracleError::NotBridgeContractAddress.into());
    }

    // Explicitly cast txid to String and ensure it meets requirements
    let txid = data.txid.clone();
    if txid.len() > MAX_TXID_LENGTH {
        msg!("TXID exceeds maximum length.");
        return Err(OracleError::InvalidTxid.into());
    }

    // Add the TXID to the monitored list
    state.monitored_txids.push(txid.clone());

    // Initialize pending_payment_account here using the txid
    let pending_payment_account = &mut ctx.accounts.pending_payment_account;
    pending_payment_account.pending_payment = PendingPayment {
        txid: txid.clone(),
        expected_amount: COST_IN_LAMPORTS_OF_ADDING_PASTEL_TXID_FOR_MONITORING,
        payment_status: PaymentStatus::Pending, // Enum, no need for casting
    };

    msg!(
        "Added Pastel TXID for Monitoring: {}",
        pending_payment_account.pending_payment.txid
    );
    Ok(())
}
