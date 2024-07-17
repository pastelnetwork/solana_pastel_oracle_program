pub mod constant;
pub mod enumeration;
pub mod error;
pub mod helper;
pub mod instructions;
pub mod state;

use crate::enumeration::{PastelTicketType, PaymentStatus, TxidStatus};
use crate::instructions::*;
use crate::state::{PastelTxStatusReport, PendingPayment};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::clock::Clock;

declare_id!("AfP1c4sFcY1FeiGjQEtyxCim8BRnw22okNbKAsH2sBsB");

#[program]
pub mod solana_pastel_oracle_program {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Initializing Oracle Contract State");
        ctx.accounts.initialize_oracle_state()?;
        // msg!("Oracle Contract State Initialized with Admin Pubkey: {:?}", admin_pubkey);

        // Logging for Reward Pool and Fee Receiving Contract Accounts
        // msg!("Reward Pool Account: {:?}", ctx.accounts.reward_pool_account.key());
        // msg!("Fee Receiving Contract Account: {:?}", ctx.accounts.fee_receiving_contract_account.key());
        msg!(
            "Temp Report Account: {:?}",
            ctx.accounts.temp_report_account.key()
        );
        msg!(
            "Contributor Data Account: {:?}",
            ctx.accounts.contributor_data_account.key()
        );
        msg!(
            "Txid Submission Counts Account: {:?}",
            ctx.accounts.txid_submission_counts_account.key()
        );
        msg!(
            "Aggregated Consensus Data Account: {:?}",
            ctx.accounts.aggregated_consensus_data_account.key()
        );

        Ok(())
    }

    pub fn reallocate_oracle_state(ctx: Context<ReallocateOracleState>) -> Result<()> {
        ReallocateOracleState::execute(ctx)
    }

    pub fn register_new_data_contributor(ctx: Context<RegisterNewDataContributor>) -> Result<()> {
        register_new_data_contributor_helper(ctx)
    }

    pub fn add_txid_for_monitoring(
        ctx: Context<AddTxidForMonitoring>,
        data: AddTxidForMonitoringData,
    ) -> Result<()> {
        add_txid_for_monitoring_helper(ctx, data)
    }

    pub fn add_pending_payment(
        ctx: Context<HandlePendingPayment>,
        txid: String,
        expected_amount: u64,
        payment_status: PaymentStatus,
    ) -> Result<()> {
        let pending_payment = PendingPayment {
            txid: txid.clone(),
            expected_amount,
            payment_status,
        };

        add_pending_payment_helper(ctx, txid, pending_payment).map_err(|e| e.into())
    }

    pub fn process_payment(ctx: Context<ProcessPayment>, txid: String, amount: u64) -> Result<()> {
        process_payment_helper(ctx, txid, amount)
    }

    pub fn submit_data_report(
        ctx: Context<SubmitDataReport>,
        txid: String,
        txid_status: TxidStatus,
        pastel_ticket_type: PastelTicketType,
        first_6_characters_hash: String,
        contributor_reward_address: Pubkey,
    ) -> Result<()> {
        msg!("In `submit_data_report` function -- Params: txid={}, txid_status={:?}, pastel_ticket_type={:?}, first_6_chars_hash={}, contributor_addr={}",
            txid, txid_status, pastel_ticket_type, first_6_characters_hash, contributor_reward_address);

        let timestamp = Clock::get()?.unix_timestamp as u64;

        let report = PastelTxStatusReport {
            txid: txid.clone(),
            txid_status,
            pastel_ticket_type: Some(pastel_ticket_type),
            first_6_characters_of_sha3_256_hash_of_corresponding_file: Some(
                first_6_characters_hash,
            ),
            timestamp,
            contributor_reward_address,
        };

        submit_data_report_helper(ctx, txid, report, contributor_reward_address)
    }

    pub fn request_reward(ctx: Context<RequestReward>, contributor_address: Pubkey) -> Result<()> {
        request_reward_helper(ctx, contributor_address)
    }

    pub fn set_bridge_contract(
        ctx: Context<SetBridgeContract>,
        bridge_contract_pubkey: Pubkey,
    ) -> Result<()> {
        SetBridgeContract::set_bridge_contract(ctx, bridge_contract_pubkey)
    }

    pub fn withdraw_funds(
        ctx: Context<WithdrawFunds>,
        reward_pool_amount: u64,
        fee_receiving_amount: u64,
    ) -> Result<()> {
        WithdrawFunds::execute(ctx, reward_pool_amount, fee_receiving_amount)
    }
}
