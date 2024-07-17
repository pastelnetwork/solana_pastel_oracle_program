use anchor_lang::prelude::*;
use super::TxidSubmissionCount;

#[account]
pub struct OracleContractState {
    pub is_initialized: bool,
    pub admin_pubkey: Pubkey,
    pub txid_submission_counts: Vec<TxidSubmissionCount>,
    pub monitored_txids: Vec<String>,
    pub reward_pool_account: Pubkey,
    pub fee_receiving_contract_account: Pubkey,
    pub txid_submission_counts_account: Pubkey,
    pub aggregated_consensus_data_account: Pubkey,
    pub bridge_contract_pubkey: Pubkey,
}
