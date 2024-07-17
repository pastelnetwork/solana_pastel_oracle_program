use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::{hash, Hash};

use crate::constant::SUBMISSION_COUNT_RETENTION_PERIOD;
use crate::state::OracleContractState;

pub fn create_seed(seed_preamble: &str, txid: &str, reward_address: &Pubkey) -> Hash {
    // Concatenate the string representations. Reward address is Base58-encoded by default.
    let preimage_string = format!("{}{}{}", seed_preamble, txid, reward_address);
    // msg!("create_seed: generated preimage string: {}", preimage_string);
    // Convert the concatenated string to bytes
    let preimage_bytes = preimage_string.as_bytes();
    // Compute hash
    let seed_hash = hash(preimage_bytes);
    // msg!("create_seed: generated seed hash: {:?}", seed_hash);
    seed_hash
}

pub fn get_report_account_pda(
    program_id: &Pubkey,
    txid: &str,
    contributor_reward_address: &Pubkey,
) -> (Pubkey, u8) {
    msg!(
        "get_report_account_pda: program_id: {}, txid: {}, contributor_reward_address: {}",
        program_id,
        txid,
        contributor_reward_address
    );
    let seed_hash = create_seed("pastel_tx_status_report", txid, contributor_reward_address);
    msg!("get_report_account_pda: seed_hash: {:?}", seed_hash);
    Pubkey::find_program_address(&[seed_hash.as_ref()], program_id)
}

pub fn cleanup_old_submission_counts(state: &mut OracleContractState) -> Result<()> {
    let current_time = Clock::get()?.unix_timestamp as u64;
    state
        .txid_submission_counts
        .retain(|count| current_time - count.last_updated < SUBMISSION_COUNT_RETENTION_PERIOD);
    Ok(())
}
