use crate::enumeration::{PastelTicketType, TxidStatus};
use anchor_lang::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, AnchorSerialize, AnchorDeserialize)]
pub struct PastelTxStatusReport {
    pub txid: String,
    pub txid_status: TxidStatus,
    pub pastel_ticket_type: Option<PastelTicketType>,
    pub first_6_characters_of_sha3_256_hash_of_corresponding_file: Option<String>,
    pub timestamp: u64,
    pub contributor_reward_address: Pubkey,
}

#[account]
pub struct PastelTxStatusReportAccount {
    pub report: PastelTxStatusReport,
}
