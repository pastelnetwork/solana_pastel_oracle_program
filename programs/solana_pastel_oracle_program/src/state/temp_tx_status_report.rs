use crate::enumeration::{PastelTicketType, TxidStatus};
use anchor_lang::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, AnchorSerialize, AnchorDeserialize)]
pub struct CommonReportData {
    pub txid: String,
    pub txid_status: TxidStatus,
    pub pastel_ticket_type: Option<PastelTicketType>,
    pub first_6_characters_of_sha3_256_hash_of_corresponding_file: Option<String>,
}

#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct SpecificReportData {
    pub contributor_reward_address: Pubkey,
    pub timestamp: u64,
    pub common_data_ref: u64, // Reference to CommonReportData
}

#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct TempTxStatusReport {
    pub common_data_ref: u64, // Index to CommonReportData in common_reports
    pub specific_data: SpecificReportData,
}

#[account]
pub struct TempTxStatusReportAccount {
    pub reports: Vec<TempTxStatusReport>,
    pub common_reports: Vec<CommonReportData>,
    pub specific_reports: Vec<SpecificReportData>,
}
