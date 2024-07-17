use anchor_lang::prelude::*;

#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct TxidSubmissionCount {
    pub txid: String,
    pub count: u32,
    pub last_updated: u64,
}

#[account]
pub struct TxidSubmissionCountsAccount {
    pub submission_counts: Vec<TxidSubmissionCount>,
}
