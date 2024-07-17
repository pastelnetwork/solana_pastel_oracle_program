use crate::constant::TXID_STATUS_VARIANT_COUNT;
use anchor_lang::prelude::*;

#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct HashWeight {
    pub hash: String,
    pub weight: i32,
}

// Struct to hold aggregated data for consensus calculation
#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct AggregatedConsensusData {
    pub txid: String,
    pub status_weights: [i32; TXID_STATUS_VARIANT_COUNT],
    pub hash_weights: Vec<HashWeight>,
    pub first_6_characters_of_sha3_256_hash_of_corresponding_file: String,
    pub last_updated: u64, // Unix timestamp indicating the last update time
}

#[account]
pub struct AggregatedConsensusDataAccount {
    pub consensus_data: Vec<AggregatedConsensusData>,
}
