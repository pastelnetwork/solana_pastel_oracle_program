use anchor_lang::prelude::*;
use anchor_lang::solana_program::entrypoint::ProgramResult;
use anchor_lang::solana_program::account_info::AccountInfo;
use anchor_lang::solana_program::sysvar::clock::Clock;
use anchor_lang::solana_program::hash::{hash, Hash};
use std::cmp;

const REGISTRATION_ENTRANCE_FEE_IN_LAMPORTS: u64 = 10_000_000; // 0.10 SOL in lamports
const MIN_REPORTS_FOR_REWARD: u32 = 10; // Data Contributor must submit at least 10 reports to be eligible for rewards
const MIN_COMPLIANCE_SCORE_FOR_REWARD: i32 = 80; // Data Contributor must have a compliance score of at least 80 to be eligible for rewards
const BASE_REWARD_AMOUNT_IN_LAMPORTS: u64 = 100_000; // 0.0001 SOL in lamports is the base reward amount, which is scaled based on the number of highly reliable contributors
const COST_IN_LAMPORTS_OF_ADDING_PASTEL_TXID_FOR_MONITORING: u64 = 100_000; // 0.0001 SOL in lamports
const PERMANENT_BAN_THRESHOLD: u32 = 100; // Number of non-consensus report submissions for permanent ban
const CONTRIBUTIONS_FOR_PERMANENT_BAN: u32 = 250; // Considered for permanent ban after 250 contributions
const TEMPORARY_BAN_THRESHOLD: u32 = 5; // Number of non-consensus report submissions for temporary ban
const CONTRIBUTIONS_FOR_TEMPORARY_BAN: u32 = 50; // Considered for temporary ban after 50 contributions
const TEMPORARY_BAN_DURATION: u64 = 604800; // Duration of temporary ban in seconds (e.g., 1 week)
const MIN_NUMBER_OF_ORACLES: usize = 10; // Minimum number of oracles to calculate consensus
const MAX_DURATION_IN_SECONDS_FROM_LAST_REPORT_SUBMISSION_BEFORE_COMPUTING_CONSENSUS: u64 = 20 * 60; // Maximum duration in seconds from last report submission for a given TXID before computing consensus (e.g., 20 minutes)
const DATA_RETENTION_PERIOD: u64 = 3 * 24 * 60 * 60; // How long to keep data in the contract state (3 days)
const SUBMISSION_COUNT_RETENTION_PERIOD: u64 = 86_400; // Number of seconds to retain submission counts (i.e., 24 hours)
const TXID_STATUS_VARIANT_COUNT: usize = 5; // Manually define the number of variants in TxidStatus

const MAX_TXID_LENGTH: usize = 64; // Maximum length of a TXID
const MAX_CONTRIBUTORS: usize = 100; // Adjust as needed
const MAX_TXID_SUBMISSION_COUNTS: usize = 100; // Adjust as needed
const MAX_MONITORED_TXIDS: usize = 100; // Adjust as needed
const MAX_AGGREGATED_CONSENSUS_DATA: usize = 100; // Adjust as needed
const MAX_TEMP_TX_STATUS_REPORTS: usize = 100; // Adjust as needed
const MAX_HASH_WEIGHTS: usize = 200; // Adjust this number based on your needs
const TRANSACTION_SIGNATURE_LENGTH: usize = 64; // Adjust as needed

#[error_code]
pub enum OracleError {
    ContributorAlreadyRegistered,
    UnregisteredOracle,
    InvalidTxid,
    InvalidFileHashLength,
    MissingPastelTicketType,
    MissingFileHash,
    RegistrationFeeNotPaid,
    NotEligibleForReward,
    NotBridgeContractAddress,
    InsufficientFunds,
    UnauthorizedWithdrawalAccount,
    InvalidPaymentAmount,
    PaymentNotFound,
    PendingPaymentAlreadyInitialized,
    AccountAlreadyInitialized,
    PendingPaymentInvalidAmount,
    InvalidPaymentStatus,
    InvalidTxidStatus,
    InvalidPastelTicketType,
    ContributorNotRegisteredOrBanned,
    MemoryAllocationFailed,
}

impl From<OracleError> for ProgramError {
    fn from(e: OracleError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

pub fn create_seed(seed_preamble: &str, txid: &str, reward_address: &Pubkey) -> Hash {
    // Concatenate the string representations. Reward address is Base58-encoded by default.
    let preimage_string = format!("{}{}{}", seed_preamble, txid, reward_address);
    msg!("create_seed: generated preimage string: {}", preimage_string);
    // Convert the concatenated string to bytes
    let preimage_bytes = preimage_string.as_bytes();
    // Compute hash
    let seed_hash = hash(preimage_bytes);
    msg!("create_seed: generated seed hash: {:?}", seed_hash);
    seed_hash
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, AnchorSerialize, AnchorDeserialize)]
pub enum TxidStatus {
    PlaceholderValue,    
    Invalid,
    PendingMining,
    MinedPendingActivation,
    MinedActivated,
}

impl Default for TxidStatus {
    fn default() -> Self {
        TxidStatus::PlaceholderValue
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, AnchorSerialize, AnchorDeserialize)]
pub enum PastelTicketType {
    Sense,
    Cascade,
    Nft,
    InferenceApi,
}


#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize, Copy)]
pub struct Contributor {
    pub reward_address: Pubkey,
    pub registration_entrance_fee_transaction_signature: [u8; TRANSACTION_SIGNATURE_LENGTH],
    pub compliance_score: i32,
    pub last_active_timestamp: u64,
    pub total_reports_submitted: u32,
    pub accurate_reports_count: u32,
    pub current_streak: u32,
    pub reliability_score: u32,
    pub consensus_failures: u32,
    pub ban_expiry: u64,
    pub is_eligible_for_rewards: bool,
    pub is_recently_active: bool,
    pub is_reliable: bool,    
}

impl Default for Contributor {
    fn default() -> Self {
        Self {
            reward_address: Pubkey::default(),
            registration_entrance_fee_transaction_signature: [0u8; TRANSACTION_SIGNATURE_LENGTH],
            compliance_score: 0,
            last_active_timestamp: 0,
            total_reports_submitted: 0,
            accurate_reports_count: 0,
            current_streak: 0,
            reliability_score: 0,
            consensus_failures: 0,
            ban_expiry: 0,
            is_eligible_for_rewards: false,
            is_recently_active: false,
            is_reliable: false,
        }
    }
}

#[account]
pub struct OracleContractState {
    pub is_initialized: bool,
    pub admin_pubkey: Pubkey,
    pub contributors: [Contributor; MAX_CONTRIBUTORS],
    pub txid_submission_counts: [TxidSubmissionCount; MAX_TXID_SUBMISSION_COUNTS],
    pub monitored_txids: [[u8; MAX_TXID_LENGTH]; MAX_MONITORED_TXIDS],
    pub aggregated_consensus_data: [AggregatedConsensusData; MAX_AGGREGATED_CONSENSUS_DATA],
    pub reward_pool_account: Pubkey,
    pub reward_pool_nonce: u8,
    pub fee_receiving_contract_account: Pubkey,
    pub fee_receiving_contract_nonce: u8,
    pub bridge_contract_pubkey: Pubkey,
    pub active_reliable_contributors_count: u32,
    pub temp_tx_status_reports: [PastelTxStatusReport; MAX_TEMP_TX_STATUS_REPORTS],
}


#[derive(Debug, Clone, PartialEq, Eq, Hash, AnchorSerialize, AnchorDeserialize, Copy)]
pub struct PastelTxStatusReport {
    pub txid: [u8; MAX_TXID_LENGTH], // 64 bytes for txid
    pub txid_status: TxidStatus,
    pub pastel_ticket_type: Option<PastelTicketType>,
    pub first_6_characters_of_sha3_256_hash_of_corresponding_file: Option<[u8; 6]>, // Using 6 bytes for the hash segment
    pub timestamp: u64,
    pub contributor_reward_address: Pubkey,
}


impl Default for PastelTxStatusReport {
    fn default() -> Self {
        Self {
            txid: [0u8; MAX_TXID_LENGTH],
            txid_status: TxidStatus::default(), // Assuming TxidStatus implements Default
            pastel_ticket_type: None,
            first_6_characters_of_sha3_256_hash_of_corresponding_file: None,
            timestamp: 0,
            contributor_reward_address: Pubkey::default(), // Assuming Pubkey implements Default
        }
    }
}

// Functions to convert between Strings and byte arrays
impl PastelTxStatusReport {


    fn string_to_fixed_array(s: &str) -> [u8; MAX_TXID_LENGTH] {
        let mut array = [0u8; MAX_TXID_LENGTH];
        let bytes = s.as_bytes();
        array[..bytes.len()].copy_from_slice(bytes);
        array
    }

    fn string_to_fixed_array_6(s: &str) -> [u8; 6] {
        let mut array = [0u8; 6];
        let bytes = s.as_bytes();
        array[..bytes.len()].copy_from_slice(bytes);
        array
    }

    // Check if the report is empty (i.e., uninitialized or default state)
    pub fn is_empty(&self) -> bool {
        self.txid == [0u8; MAX_TXID_LENGTH]
    }    
}


#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize, Copy)]
pub struct TxidSubmissionCount {
    pub txid: [u8; MAX_TXID_LENGTH], // 64 bytes for txid
    pub count: u32,
    pub last_updated: u64,
}


impl Default for TxidSubmissionCount {
    fn default() -> Self {
        Self {
            txid: [0u8; MAX_TXID_LENGTH], // Default value for txid
            count: 0,
            last_updated: 0,
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub struct PendingPayment {
    pub txid: [u8; MAX_TXID_LENGTH], // 64 bytes for txid,
    pub expected_amount: u64,
    pub payment_status: PaymentStatus,
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub enum PaymentStatus {
    Pending,
    Received
}

#[account]
pub struct RewardPool {
    // Since this account is only used for holding and transferring SOL, no fields are necessary.
}

#[account]
pub struct FeeReceivingContract {
    // Since this account is only used for holding and transferring SOL, no fields are necessary.
}

#[account]
pub struct PastelTxStatusReportAccount {
    pub report: PastelTxStatusReport,
}

#[derive(Accounts)]
#[instruction(txid: String, reward_address: Pubkey)]
pub struct SubmitDataReport<'info> {
    #[account(
        init_if_needed,
        payer = user,
        seeds = [create_seed("pastel_tx_status_report", &txid, &user.key()).as_ref()],
        bump,
        space = 8 + (64 + 1 + 2 + 7 + 8 + 32 + 128) // Discriminator +  txid String (max length of 64) + txid_status + pastel_ticket_type + first_6_characters_of_sha3_256_hash_of_corresponding_file + timestamp + contributor_reward_address + cushion
    )]
    pub report_account: Account<'info, PastelTxStatusReportAccount>,

    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[event]
pub struct DataReportSubmitted {
    pub contributor: Pubkey,
    pub txid: [u8; MAX_TXID_LENGTH], // 64 bytes for txid,
    pub status: TxidStatus,
    pub timestamp: u64,
}


fn update_submission_count(state: &mut OracleContractState, txid: [u8; MAX_TXID_LENGTH]) -> Result<()> {
    // Get the current timestamp
    let current_timestamp_u64 = Clock::get()?.unix_timestamp as u64;

    // Initialize a flag to track if txid was found
    let mut found = false;

    // Iterate over txid_submission_counts to find or create a submission count
    for count in state.txid_submission_counts.iter_mut() {
        // Check if this entry is the one we're looking for
        if count.txid == txid {
            // Update the existing count
            count.count += 1;
            count.last_updated = current_timestamp_u64;
            found = true;
            break;
        }
    }

    // If the txid was not found, look for an empty slot to add a new entry
    if !found {
        if let Some(count) = state.txid_submission_counts.iter_mut().find(|c| c.txid == [0u8; MAX_TXID_LENGTH]) {
            // Found an empty slot, insert the new count
            *count = TxidSubmissionCount {
                txid,
                count: 1,
                last_updated: current_timestamp_u64,
            };
        } else {
            // No empty slots found, return an error
            return Err(OracleError::MemoryAllocationFailed.into());
        }
    }

    Ok(())
}


pub fn get_report_account_pda(
    program_id: &Pubkey, 
    txid: &[u8; MAX_TXID_LENGTH], 
    contributor_reward_address: &Pubkey
) -> (Pubkey, u8) {
    // Convert the byte array txid to a string
    let txid_str = String::from_utf8_lossy(txid);

    msg!("get_report_account_pda: program_id: {}, txid: {}, contributor_reward_address: {}", program_id, txid_str, contributor_reward_address);
    // Use the string representation of txid for seed generation
    let seed_hash = create_seed("pastel_tx_status_report", &txid_str, contributor_reward_address);
    msg!("get_report_account_pda: seed_hash: {:?}", seed_hash);

    Pubkey::find_program_address(&[seed_hash.as_ref()], program_id)
}

fn get_aggregated_data<'a>(state: &'a OracleContractState, txid: &[u8; MAX_TXID_LENGTH]) -> Option<&'a AggregatedConsensusData> {
    state.aggregated_consensus_data.iter()
        .find(|data| data.txid == *txid)
}

fn compute_consensus(aggregated_data: &AggregatedConsensusData) -> (TxidStatus, [u8; 6]) {
    let consensus_status = aggregated_data.status_weights.iter().enumerate().max_by_key(|&(_, weight)| weight)
        .map(|(index, _)| usize_to_txid_status(index).unwrap_or(TxidStatus::Invalid)).unwrap();

    let consensus_hash = aggregated_data.hash_weights.iter().max_by_key(|hash_weight| hash_weight.weight)
        .map(|hash_weight| hash_weight.hash.unwrap_or([0u8; 6])) // Return the hash array or a default one if not present
        .unwrap_or([0u8; 6]);

    (consensus_status, consensus_hash)
}

fn calculate_consensus_and_cleanup(
    state: &mut OracleContractState,
    txid: [u8; MAX_TXID_LENGTH],
) -> Result<()> {
    let current_timestamp = Clock::get()?.unix_timestamp as u64;

    // Assuming `get_aggregated_data` and `compute_consensus` are updated to work with the new data types
    let (consensus_status, consensus_hash) = get_aggregated_data(state, &txid)
        .map(|data| compute_consensus(data))
        .unwrap_or((TxidStatus::Invalid, [0u8; 6])); // Replace String::new() with a default hash array

    let mut updated_contributors = Vec::new();
    let mut contributor_count = 0;

    for report in state.temp_tx_status_reports.iter() {
        if report.txid == txid && !updated_contributors.contains(&report.contributor_reward_address) {
            if let Some(contributor) = state.contributors.iter_mut().find(|c| c.reward_address == report.contributor_reward_address) {
                let is_accurate = report.txid_status == consensus_status &&
                    report.first_6_characters_of_sha3_256_hash_of_corresponding_file.map_or(false, |hash| hash == consensus_hash);
                update_activity_and_compliance_helper(contributor, current_timestamp, is_accurate)?;
                updated_contributors.push(report.contributor_reward_address);
            }
            contributor_count += 1;
        }
    }

    // Convert txid to string for logging
    let txid_str = String::from_utf8_lossy(&txid);
    msg!("Consensus Reached: TXID: {}, Status: {:?}, Hash: {:?}, Contributors: {}", txid_str, consensus_status, consensus_hash, contributor_count);

    emit!(ConsensusReachedEvent {
        txid,
        status: consensus_status,
        hash: Some(consensus_hash),
        number_of_contributors_included: contributor_count as u32,
    });
    state.assess_and_apply_bans(current_timestamp);

    // Pruning logic for fixed-size arrays
    let mut new_temp_tx_status_reports = [PastelTxStatusReport::default(); MAX_TEMP_TX_STATUS_REPORTS];
    let mut new_aggregated_consensus_data = [AggregatedConsensusData::default(); MAX_AGGREGATED_CONSENSUS_DATA];

    let mut new_report_count = 0;
    let mut new_aggregated_count = 0;

    // Retain logic for temp_tx_status_reports
    for report in state.temp_tx_status_reports.iter() {
        if report.txid != txid && current_timestamp - report.timestamp < DATA_RETENTION_PERIOD {
            new_temp_tx_status_reports[new_report_count] = *report;
            new_report_count += 1;
        }
    }

    // Retain logic for aggregated_consensus_data
    for data in state.aggregated_consensus_data.iter() {
        if current_timestamp - data.last_updated < DATA_RETENTION_PERIOD {
            new_aggregated_consensus_data[new_aggregated_count] = *data;
            new_aggregated_count += 1;
        }
    }

    state.temp_tx_status_reports = new_temp_tx_status_reports;
    state.aggregated_consensus_data = new_aggregated_consensus_data;

    Ok(())
}


// Function to update hash weight in a fixed-size array
fn update_hash_weight_fixed_array(hash_weights: &mut [HashWeight; MAX_HASH_WEIGHTS], hash: &[u8; 6], weight: i32) {
    let mut found = false;

    for hash_weight in hash_weights.iter_mut() {
        if hash_weight.hash == Some(*hash) {
            hash_weight.weight += weight;
            found = true;
            break;
        }
    }

    if !found {
        add_hash_weight_fixed_array(hash_weights, hash, weight);
    }
}

// Function to add a new hash weight in the first empty slot of the fixed-size array
fn add_hash_weight_fixed_array(hash_weights: &mut [HashWeight; MAX_HASH_WEIGHTS], hash: &[u8; 6], weight: i32) {
    for hash_weight in hash_weights.iter_mut() {
        if hash_weight.is_empty() {
            *hash_weight = HashWeight { hash: Some(*hash), weight };
            break;
        }
    }
}

fn aggregate_consensus_data(
    state: &mut OracleContractState,
    report: &PastelTxStatusReport,
    weight: u32,
    txid: [u8; MAX_TXID_LENGTH],
) -> Result<()> {
    let weight_i32 = weight as i32;
    let current_timestamp = Clock::get()?.unix_timestamp as u64;

    let mut data_found = false;
    for data_entry in &mut state.aggregated_consensus_data {
        if data_entry.txid == txid {
            // Update existing data
            data_entry.status_weights[report.txid_status as usize] += weight_i32;
            if let Some(hash) = report.first_6_characters_of_sha3_256_hash_of_corresponding_file {
                // Update or add new hash weight
                update_hash_weight_fixed_array(&mut data_entry.hash_weights, &hash, weight_i32);
            }
            data_entry.last_updated = current_timestamp;
            data_found = true;
            break;
        }
    }

    if !data_found {
        // Find an empty slot or placeholder in the fixed-size array
        let mut slot_found = false;
        for data_entry in &mut state.aggregated_consensus_data {
            if data_entry.is_empty() {
                *data_entry = AggregatedConsensusData {
                    txid,
                    status_weights: [0; TXID_STATUS_VARIANT_COUNT],
                    hash_weights: [HashWeight::default(); MAX_HASH_WEIGHTS],
                    last_updated: current_timestamp,
                };
                data_entry.status_weights[report.txid_status as usize] += weight_i32;
                if let Some(hash) = report.first_6_characters_of_sha3_256_hash_of_corresponding_file {
                    // Add hash weight to the first empty slot
                    add_hash_weight_fixed_array(&mut data_entry.hash_weights, &hash, weight_i32);
                }
                slot_found = true;
                break;
            }
        }

        if !slot_found {
            msg!("Failed to aggregate consensus data: No available slot in aggregated_consensus_data");
            return Err(OracleError::MemoryAllocationFailed.into());
        }
    }

    Ok(())
}


pub fn submit_data_report_helper(
    ctx: Context<SubmitDataReport>, 
    txid: [u8; MAX_TXID_LENGTH], 
    report: PastelTxStatusReport,
    contributor_reward_address: Pubkey
) -> ProgramResult {

    // Validate the data report before any processing
    validate_data_contributor_report(&report)?;
    
    // First, handle the immutable borrow
    let compliance_score;
    let is_banned;
    {
        let state = &ctx.accounts.oracle_contract_state;
        if let Some(contributor) = state.contributors.iter().find(|c| c.reward_address == contributor_reward_address) {
            compliance_score = contributor.compliance_score;
            is_banned = contributor.calculate_is_banned(Clock::get()?.unix_timestamp as u64);
        } else {
            return Err(OracleError::ContributorNotRegisteredOrBanned.into());
        }
    }
    // Early return if the contributor is banned
    if is_banned {
        return Err(OracleError::ContributorNotRegisteredOrBanned.into());
    }

    // Now handle the mutable borrow
    let state = &mut ctx.accounts.oracle_contract_state;

    msg!("Adding new Data Report to contract state: {:?}", report);

    // Handle adding the report to temp_tx_status_reports (assuming it's now a fixed-size array)
    let report_added = {
        let mut added = false;
        for existing_report in state.temp_tx_status_reports.iter_mut() {
            if existing_report.is_empty() { // is_empty method checks for a default/empty report
                *existing_report = report.clone();
                added = true;
                break;
            }
        }
        added
    };

    if !report_added {
        msg!("Failed to add new report: No available space in temp_tx_status_reports");
        return Err(OracleError::MemoryAllocationFailed.into());
    }

    ctx.accounts.report_account.report = report.clone();
    update_submission_count(state, txid)?;

    msg!("New size of temp_tx_status_reports in bytes: {}", state.temp_tx_status_reports.len() * std::mem::size_of::<PastelTxStatusReport>());

    let weight = normalize_compliance_score(compliance_score) as u32;
    aggregate_consensus_data(state, &report, weight, txid)?;
    if should_calculate_consensus(state, txid)? {
        calculate_consensus_and_cleanup(state, txid)?;
    }
    cleanup_old_submission_counts(state)?;
    
    Ok(())
}


#[derive(Accounts)]
#[instruction(txid: String)]
pub struct HandleConsensus<'info> {

    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[account]
pub struct PendingPaymentAccount {
    pub pending_payment: PendingPayment,
}

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
    txid: [u8; MAX_TXID_LENGTH], // Accepting txid as [u8; 64]
    pending_payment: PendingPayment
) -> ProgramResult {
    let pending_payment_account = &mut ctx.accounts.pending_payment_account;

    // Ensure the account is being initialized for the first time to avoid re-initialization
    if !pending_payment_account.pending_payment.txid.is_empty() && pending_payment_account.pending_payment.txid != txid {
        return Err(OracleError::PendingPaymentAlreadyInitialized.into());
    }

    // Ensure txid is correct and other fields are properly set
    if pending_payment.txid != txid {
        return Err(OracleError::InvalidTxid.into());
    }

    // Store the pending payment in the account
    pending_payment_account.pending_payment = pending_payment;

    // Convert txid to String for logging
    let txid_str = String::from_utf8_lossy(&txid);

    msg!("Pending payment account initialized: TXID: {}, Expected Amount: {}, Status: {:?}", 
        txid_str, 
        pending_payment_account.pending_payment.expected_amount, 
        pending_payment_account.pending_payment.payment_status);

    Ok(())
}


#[derive(Accounts)]
#[instruction(admin_pubkey: Pubkey)]
pub struct Initialize<'info> {
    #[account(init, payer = user, space = 10_240)] // Adjusted space
    pub oracle_contract_state: Account<'info, OracleContractState>,
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        init,
        seeds = [b"reward_pool"],
        bump,
        payer = user,
        space = 8 + 1024 // Adjust the space as needed
    )]
    pub reward_pool_account: Account<'info, RewardPool>,

    #[account(
        init,
        seeds = [b"fee_receiving_contract"],
        bump,
        payer = user,
        space = 8 + 1024 // Adjust the space as needed
    )]
    pub fee_receiving_contract_account: Account<'info, FeeReceivingContract>,

    // System program is needed for account creation
    pub system_program: Program<'info, System>,    
}

impl<'info> Initialize<'info> {
    pub fn initialize_oracle_state(&mut self, admin_pubkey: Pubkey) -> Result<()> {
        msg!("Setting up Oracle Contract State");

        let state = &mut self.oracle_contract_state;
        // Ensure the oracle_contract_state is not already initialized
        if state.is_initialized {
            return Err(OracleError::AccountAlreadyInitialized.into());
        }

        state.is_initialized = true;
        state.admin_pubkey = admin_pubkey;
        msg!("Admin Pubkey set to: {:?}", admin_pubkey);


        // Initialize arrays with default values
        state.contributors = [Contributor::default(); MAX_CONTRIBUTORS];
        msg!("Contributors array initialized");

        state.txid_submission_counts = [TxidSubmissionCount::default(); MAX_TXID_SUBMISSION_COUNTS];
        msg!("Txid Submission Counts array initialized");

        state.monitored_txids = [[0u8; MAX_TXID_LENGTH]; MAX_MONITORED_TXIDS];
        msg!("Monitored Txids array initialized");

        state.aggregated_consensus_data = [AggregatedConsensusData::default(); MAX_AGGREGATED_CONSENSUS_DATA];
        msg!("Aggregated Consensus Data array initialized");

        state.bridge_contract_pubkey = Pubkey::default();
        msg!("Bridge Contract Pubkey set to default");

        state.active_reliable_contributors_count = 0;
        msg!("Active Reliable Contributors Count set to 0");

        msg!("Oracle Contract State Initialization Complete");
        Ok(())
    }
}


#[derive(Accounts)]
pub struct ReallocateOracleState<'info> {
    #[account(mut, has_one = admin_pubkey)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    pub admin_pubkey: Signer<'info>,
    pub system_program: Program<'info, System>,
}

impl<'info> ReallocateOracleState<'info> {
    pub fn execute(ctx: Context<ReallocateOracleState>) -> Result<()> {
        let oracle_contract_state = &mut ctx.accounts.oracle_contract_state;

        // Calculate new size; add 10,240 bytes for each reallocation
        // Ensure not to exceed 100KB total size
        let current_size = oracle_contract_state.to_account_info().data_len();
        let additional_space = 10_240; // Increment size
        let max_size = 100 * 1024; // 100KB
        let new_size = std::cmp::min(current_size + additional_space, max_size);

        // Perform reallocation
        oracle_contract_state.to_account_info().realloc(new_size, false)?;

        msg!("OracleContractState reallocated to new size: {}", new_size);
        Ok(())
    }
}


#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize, Default, Copy)]
pub struct HashWeight {
    pub hash: Option<[u8; 6]>, // Using 6 bytes for the hash segment,
    pub weight: i32,
}

impl HashWeight {
    // Check if the HashWeight is empty (default state)
    fn is_empty(&self) -> bool {
        self.hash.is_none() && self.weight == 0
    }
}


// Struct to hold aggregated data for consensus calculation
#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize, Copy)]
pub struct AggregatedConsensusData {
    pub txid: [u8; MAX_TXID_LENGTH], // 64 bytes for txid
    pub status_weights: [i32; TXID_STATUS_VARIANT_COUNT],
    pub hash_weights: [HashWeight; MAX_HASH_WEIGHTS], // Fixed-size array
    pub last_updated: u64, // Unix timestamp indicating the last update time
}

impl AggregatedConsensusData {
    pub fn is_empty(&self) -> bool {
        self.txid == [0u8; MAX_TXID_LENGTH] &&
        self.status_weights.iter().all(|&weight| weight == 0) &&
        self.hash_weights.is_empty() &&
        self.last_updated == 0
    }
}

impl Default for AggregatedConsensusData {
    fn default() -> Self {
        Self {
            txid: [0u8; MAX_TXID_LENGTH],
            status_weights: [0; TXID_STATUS_VARIANT_COUNT],
            hash_weights: [HashWeight::default(); MAX_HASH_WEIGHTS],
            last_updated: 0,
        }
    }
}

#[derive(Accounts)]
pub struct UpdateActivityAndCompliance<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
}

pub fn update_activity_and_compliance(
    ctx: Context<UpdateActivityAndCompliance>, 
    contributor_address: Pubkey, // Passed as an argument
    current_timestamp: u64, 
    is_accurate: bool
) -> Result<()> {
    let oracle_state = &mut ctx.accounts.oracle_contract_state;

    // Find the contributor in the oracle state
    if let Some(contributor) = oracle_state.contributors.iter_mut().find(|c| c.reward_address == contributor_address) {
        update_activity_and_compliance_helper(contributor, current_timestamp, is_accurate)?;
    } else {
        msg!("Contributor not found: {}", contributor_address);
        return Err(OracleError::UnregisteredOracle.into());
    }

    Ok(())
}

#[derive(Accounts)]
pub struct RequestReward<'info> {
    #[account(mut)]
    pub reward_pool_account: Account<'info, RewardPool>,
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    pub system_program: Program<'info, System>,
}

#[event]
pub struct RewardEvent {
    pub contributor: Pubkey,
    pub amount: u64,
    pub status: String,
}

pub fn request_reward_helper(ctx: Context<RequestReward>, contributor_address: Pubkey) -> Result<()> {
    let oracle_state = &mut ctx.accounts.oracle_contract_state;

    // Temporarily store reward eligibility and amount
    let mut reward_amount = 0;
    let mut is_reward_valid = false;

    // Find the contributor in the oracle state and check eligibility
    if let Some(contributor) = oracle_state.contributors.iter().find(|c| c.reward_address == contributor_address) {
        let current_unix_timestamp = Clock::get()?.unix_timestamp as u64;
        let is_eligible_for_rewards = contributor.is_eligible_for_rewards;
        let is_banned = contributor.calculate_is_banned(current_unix_timestamp);

        if is_eligible_for_rewards && !is_banned {
            reward_amount = BASE_REWARD_AMOUNT_IN_LAMPORTS; // Adjust based on your logic
            is_reward_valid = true;
        }
    } else {
        msg!("Contributor not found: {}", contributor_address);
        return Err(OracleError::UnregisteredOracle.into());
    }

    // Handle reward transfer and event emission after determining eligibility
    if is_reward_valid {
        // Transfer the reward from the reward pool to the contributor
        **ctx.accounts.reward_pool_account.to_account_info().lamports.borrow_mut() -= reward_amount;
        **ctx.accounts.oracle_contract_state.to_account_info().lamports.borrow_mut() += reward_amount;

        // Emit event for valid reward request
        emit!(RewardEvent {
            contributor: contributor_address,
            amount: reward_amount,
            status: "Valid Reward Paid".to_string(),
        });

        msg!("Paid out Valid Reward Request: Contributor: {}, Amount: {}", contributor_address, reward_amount);
    } else {
        // Emit event for invalid reward request
        emit!(RewardEvent {
            contributor: contributor_address,
            amount: 0,
            status: "Invalid Reward Request".to_string(),
        });

        msg!("Invalid Reward Request: Contributor: {}", contributor_address);
        return Err(OracleError::NotEligibleForReward.into());
    }

    Ok(())
}


#[derive(Accounts)]
pub struct RegisterNewDataContributor<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    /// CHECK: Manual checks are performed in the instruction to ensure the contributor_account is valid and safe to use.
    #[account(mut, signer)]
    pub contributor_account: AccountInfo<'info>,
    
    #[account(mut)]
    pub reward_pool_account: Account<'info, RewardPool>,
    #[account(mut)]
    pub fee_receiving_contract_account: Account<'info, FeeReceivingContract>,
}

#[event]
pub struct ContributorRegisteredEvent {
    pub address: Pubkey,
    pub timestamp: u64,
}

pub fn register_new_data_contributor_helper(ctx: Context<RegisterNewDataContributor>) -> Result<()> {
    let state = &mut ctx.accounts.oracle_contract_state;
    msg!("Initiating new contributor registration: {}", ctx.accounts.contributor_account.key());

    // Check if the contributor is already registered
    if state.contributors.iter().any(|c| c.reward_address == *ctx.accounts.contributor_account.key) {
        msg!("Registration failed: Contributor already registered: {}", ctx.accounts.contributor_account.key);
        return Err(OracleError::ContributorAlreadyRegistered.into());
    }

    // Retrieve mutable references to the lamport balance
    let fee_receiving_account_info = ctx.accounts.fee_receiving_contract_account.to_account_info();
    let mut fee_receiving_account_lamports = fee_receiving_account_info.lamports.borrow_mut();

    let reward_pool_account_info = ctx.accounts.reward_pool_account.to_account_info();
    let mut reward_pool_account_lamports = reward_pool_account_info.lamports.borrow_mut();

    // Check if the fee_receiving_contract_account received the registration fee
    if **fee_receiving_account_lamports < REGISTRATION_ENTRANCE_FEE_IN_LAMPORTS as u64 {
        return Err(OracleError::RegistrationFeeNotPaid.into());
    }

    msg!("Registration fee verified. Attempting to register new contributor {}", ctx.accounts.contributor_account.key);

    // Deduct the registration fee from the fee_receiving_contract_account and add it to the reward pool account
    **fee_receiving_account_lamports -= REGISTRATION_ENTRANCE_FEE_IN_LAMPORTS as u64;
    **reward_pool_account_lamports += REGISTRATION_ENTRANCE_FEE_IN_LAMPORTS as u64;

    let last_active_timestamp = Clock::get()?.unix_timestamp as u64;
    
    // Create and add the new contributor
    let new_contributor = Contributor {
        reward_address: *ctx.accounts.contributor_account.key,
        registration_entrance_fee_transaction_signature: [0; TRANSACTION_SIGNATURE_LENGTH],
        compliance_score: 0, // Initial compliance score
        last_active_timestamp, // Set the last active timestamp to the current time
        total_reports_submitted: 0, // Initially, no reports have been submitted
        accurate_reports_count: 0, // Initially, no accurate reports
        current_streak: 0, // No streak at the beginning
        reliability_score: 0, // Initial reliability score
        consensus_failures: 0, // No consensus failures at the start
        ban_expiry: 0, // No ban initially set
        is_eligible_for_rewards: false, // Initially not eligible for rewards
        is_recently_active: false, // Initially not considered active
        is_reliable: false, // Initially not considered reliable
    };

    // Find an empty slot in the contributors array to insert the new contributor
    let mut inserted = false;
    for contributor in state.contributors.iter_mut() {
        if contributor.reward_address == Pubkey::default() { // Assumes default means uninitialized
            *contributor = new_contributor;
            inserted = true;
            break;
        }
    }

    if !inserted {
        msg!("Registration failed: No available slot for new contributor");
        return Err(OracleError::MemoryAllocationFailed.into());
    }
    // Emit event for new contributor registration
    emit!(ContributorRegisteredEvent {
        address: *ctx.accounts.contributor_account.key,
        timestamp: last_active_timestamp,
    });

    // Logging for debug purposes
    msg!("New Contributor successfully Registered: Address: {}, Timestamp: {}", ctx.accounts.contributor_account.key, last_active_timestamp);
    Ok(())
}


#[event]
pub struct ContributorUpdatedEvent {
    pub reward_address: Pubkey,
    pub last_active_timestamp: u64,
    pub total_reports_submitted: u32,
    pub accurate_reports_count: u32,
    pub current_streak: u32,
    pub compliance_score: i32,
    pub reliability_score: u32,
    pub consensus_failures: u32,
    pub ban_expiry: u64,
    pub is_recently_active: bool,
    pub is_reliable: bool,
    pub is_eligible_for_rewards: bool,
}


pub fn update_activity_and_compliance_helper(
    contributor: &mut Contributor, 
    current_timestamp: u64, 
    is_accurate: bool,
) -> Result<()> {
    msg!("Updating Contributor: {}, Is Accurate Report: {}", contributor.reward_address, is_accurate);

    // Calculating time difference
    let time_diff = current_timestamp.saturating_sub(contributor.last_active_timestamp);
    let days_inactive = time_diff as f32 / 86_400.0; // 86,400 seconds in a day
    msg!("Time Diff: {}, Days Inactive: {}", time_diff, days_inactive);

    // Calculating progressive scaling and time weight
    // Ensure total_reports_submitted is greater than zero before calculating progressive scaling
    let progressive_scaling = if contributor.total_reports_submitted > 0 {
        1.0 / (1.0 + (contributor.total_reports_submitted as f32 * 0.01).log10()).max(0.0).min(1.0)
    } else {
        1.0 // Default value when there are no reports submitted yet
    };
    let time_weight = (2.0_f32).powf(-days_inactive / 30.0); // Half-life of 30 days

    let base_score_increment = 10;
    let score_increment = (base_score_increment as f32 * progressive_scaling * time_weight) as i32;

    let score_decrement = 5; // Fixed negative value for inaccuracies

    // Streak Bonus
    let streak_bonus = if is_accurate { cmp::min(contributor.current_streak / 10, 5) as i32 } else { 0 };

    msg!("Progressive Scaling: {}, Time Weight: {}, Base Score Increment: {}, Score Increment: {}, Score Decrement: {}, Streak Bonus: {}",
        progressive_scaling, time_weight, base_score_increment, score_increment, score_decrement, streak_bonus);

    if is_accurate {
        contributor.accurate_reports_count += 1;
        contributor.current_streak += 1;
        contributor.compliance_score += score_increment + streak_bonus; // Increase score for accurate report + streak bonus
    } else {
        contributor.current_streak = 0; // Reset streak on inaccurate report
        contributor.compliance_score -= score_decrement; // Decrease score for inaccurate report
    }

    // Ensuring compliance score is within bounds
    contributor.compliance_score = cmp::min(cmp::max(contributor.compliance_score, -100), 100);

    // Ensure total_reports_submitted is greater than zero before calculating reliability score
    contributor.reliability_score = if contributor.total_reports_submitted > 0 {
        ((contributor.accurate_reports_count as f32 / contributor.total_reports_submitted as f32) * 100.0).min(100.0) as u32
    } else {
        0 // Default value when there are no reports submitted yet
    };

    msg!("Final Compliance Score: {}, Reliability Score: {}", contributor.compliance_score, contributor.reliability_score);

    // Updating bans and statuses
    if !is_accurate || contributor.consensus_failures > 0 {
        update_bans_and_statuses(contributor, current_timestamp, is_accurate);
    }

    // Updating statuses
    contributor.is_recently_active = contributor.calculate_is_recently_active(current_timestamp);
    contributor.is_reliable = contributor.calculate_is_reliable();
    contributor.is_eligible_for_rewards = contributor.calculate_is_eligible_for_rewards();

    // Emitting event for contributor update
    emit!(ContributorUpdatedEvent {
        reward_address: contributor.reward_address,
        last_active_timestamp: contributor.last_active_timestamp,
        total_reports_submitted: contributor.total_reports_submitted,
        accurate_reports_count: contributor.accurate_reports_count,
        current_streak: contributor.current_streak,
        compliance_score: contributor.compliance_score,
        reliability_score: contributor.reliability_score,
        consensus_failures: contributor.consensus_failures,
        ban_expiry: contributor.ban_expiry,
        is_recently_active: contributor.is_recently_active,
        is_reliable: contributor.is_reliable,
        is_eligible_for_rewards: contributor.is_eligible_for_rewards,
    });

    msg!("Contributor Update Complete: {}", contributor.reward_address);

    Ok(())
}


pub fn update_bans_and_statuses(contributor: &mut Contributor, current_timestamp: u64, is_accurate: bool) {
    if !is_accurate {
        contributor.consensus_failures += 1;
        msg!("Incremented Consensus Failures: {}", contributor.consensus_failures);
        if contributor.total_reports_submitted <= CONTRIBUTIONS_FOR_TEMPORARY_BAN && contributor.consensus_failures % TEMPORARY_BAN_THRESHOLD == 0 {
            contributor.ban_expiry = current_timestamp + TEMPORARY_BAN_DURATION;
            msg!("Temporary ban applied. Ban Expiry: {}", contributor.ban_expiry);
        } else if contributor.total_reports_submitted >= CONTRIBUTIONS_FOR_PERMANENT_BAN && contributor.consensus_failures >= PERMANENT_BAN_THRESHOLD {
            contributor.ban_expiry = u64::MAX;
            msg!("Permanent ban applied.");
        }
    }
    contributor.is_recently_active = contributor.calculate_is_recently_active(current_timestamp);
    contributor.is_reliable = contributor.calculate_is_reliable();
    contributor.is_eligible_for_rewards = contributor.calculate_is_eligible_for_rewards();
}


#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct AddTxidForMonitoringData {
    pub txid: String, // Use String for txid
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

#[event]
pub struct TxidAddedForMonitoringEvent {
    pub txid: String, // Use String for txid
    pub expected_amount: u64,
}


pub fn add_txid_for_monitoring_helper(ctx: Context<AddTxidForMonitoring>, data: AddTxidForMonitoringData) -> Result<()> {
    let state = &mut ctx.accounts.oracle_contract_state;

    if ctx.accounts.caller.key != &state.bridge_contract_pubkey {
        return Err(OracleError::NotBridgeContractAddress.into());
    }

    // Check the length of txid
    if data.txid.len() > MAX_TXID_LENGTH {
        msg!("TXID exceeds maximum length.");
        return Err(OracleError::InvalidTxid.into());
    }

    // Convert txid to fixed-size byte array for internal use
    let txid_bytes = PastelTxStatusReport::string_to_fixed_array(&data.txid);

    // Find an empty slot in the monitored_txids array to insert the new txid
    let mut inserted = false;
    for monitored_txid in state.monitored_txids.iter_mut() {
        if monitored_txid.iter().all(|&byte| byte == 0) {
            *monitored_txid = txid_bytes;
            inserted = true;
            break;
        }
    }
    if !inserted {
        msg!("No available slot to monitor new TXID");
        return Err(OracleError::MemoryAllocationFailed.into());
    }


    // Initialize pending_payment_account here using the txid
    let pending_payment_account = &mut ctx.accounts.pending_payment_account;
    pending_payment_account.pending_payment = PendingPayment {
        txid: txid_bytes,
        expected_amount: COST_IN_LAMPORTS_OF_ADDING_PASTEL_TXID_FOR_MONITORING,
        payment_status: PaymentStatus::Pending,
    };

    // Emit an event for adding TXID for monitoring
    emit!(TxidAddedForMonitoringEvent {
        txid: data.txid.clone(),
        expected_amount: COST_IN_LAMPORTS_OF_ADDING_PASTEL_TXID_FOR_MONITORING,
    });

    msg!("Added Pastel TXID for Monitoring: {}", data.txid);

    Ok(())
}


#[derive(Accounts)]
pub struct ProcessPastelTxStatusReport<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    /// CHECK: Manual checks are performed in the instruction to ensure the contributor is valid and authorized. This includes verifying signatures and other relevant validations.
    #[account(mut, signer)]
    pub contributor: AccountInfo<'info>,
    // You can add other accounts as needed
}

pub fn should_calculate_consensus(state: &OracleContractState, txid_bytes: [u8; MAX_TXID_LENGTH]) -> Result<bool> {
    // Retrieve the count of submissions and last updated timestamp for the given txid
    let (submission_count, last_updated) = state.txid_submission_counts.iter()
        .find(|c| c.txid == txid_bytes)
        .map(|c| (c.count, c.last_updated))
        .unwrap_or((0, 0));

    // Use the cached count of active reliable contributors
    let active_reliable_contributors_count = state.active_reliable_contributors_count;

    // Define the minimum number of reports required before attempting to calculate consensus
    let min_reports_for_consensus = std::cmp::max(MIN_NUMBER_OF_ORACLES, active_reliable_contributors_count.try_into().unwrap());

    // Check if the minimum threshold of reports is met
    let min_threshold_met = submission_count >= min_reports_for_consensus as u32;

    // Get the current unix timestamp from the Solana clock
    let current_unix_timestamp = Clock::get()?.unix_timestamp as u64;

    // Check if N minutes have elapsed since the last update
    let max_waiting_period_elapsed_for_txid = current_unix_timestamp - last_updated >= MAX_DURATION_IN_SECONDS_FROM_LAST_REPORT_SUBMISSION_BEFORE_COMPUTING_CONSENSUS;

    // Calculate consensus if minimum threshold is met or if N minutes have passed with at least MIN_NUMBER_OF_ORACLES reports
    Ok(min_threshold_met || (max_waiting_period_elapsed_for_txid && submission_count >= MIN_NUMBER_OF_ORACLES as u32))
}


pub fn cleanup_old_submission_counts(state: &mut OracleContractState) -> Result<()> {
    let current_time = Clock::get()?.unix_timestamp as u64;
    
    for count in state.txid_submission_counts.iter_mut() {
        // Check if the submission count is older than the retention period
        if current_time - count.last_updated >= SUBMISSION_COUNT_RETENTION_PERIOD {
            // Mark this count as invalid or reset its fields
            *count = TxidSubmissionCount::default(); // Reset to default if it's no longer valid
        }
    }

    Ok(())
}

// Normalize the compliance score to a positive range, e.g., [0, 100]
pub fn normalize_compliance_score(score: i32) -> i32 {
    let max_score = 100;
    let min_score = -100;
    // Adjust score to be in the range [0, 200]
    let adjusted_score = score - min_score;
    // Scale down to be in the range [0, 100]
    (adjusted_score * max_score) / (max_score - min_score)
}

pub fn usize_to_txid_status(index: usize) -> Option<TxidStatus> {
    match index {
        0 => Some(TxidStatus::PlaceholderValue),
        1 => Some(TxidStatus::Invalid),
        2 => Some(TxidStatus::PendingMining),
        3 => Some(TxidStatus::MinedPendingActivation),
        4 => Some(TxidStatus::MinedActivated),
        _ => None,
    }
}

#[event]
pub struct ConsensusReachedEvent {
    pub txid: [u8; MAX_TXID_LENGTH], // 64 bytes for txid,
    pub status: TxidStatus,
    pub hash: Option<[u8; 6]>, // Using 6 bytes for the hash segment,
    pub number_of_contributors_included: u32,
}


// Function to handle the submission of Pastel transaction status reports
pub fn validate_data_contributor_report(report: &PastelTxStatusReport) -> Result<()> {
    // Check if TXID is empty (i.e., all zeros)
    if report.txid.iter().all(|&byte| byte == 0) {
        msg!("Error: InvalidTxid (TXID is empty)");
        return Err(OracleError::InvalidTxid.into());
    } 
    // Simplified TXID status validation
    if !matches!(report.txid_status, TxidStatus::MinedActivated | TxidStatus::MinedPendingActivation | TxidStatus::PendingMining | TxidStatus::Invalid) {
        return Err(OracleError::InvalidTxidStatus.into());
    }
    // Direct return in case of missing data, reducing nested if conditions
    if report.pastel_ticket_type.is_none() {
        msg!("Error: Missing Pastel Ticket Type");
        return Err(OracleError::MissingPastelTicketType.into());
    }
    // Hash validation
    if let Some(hash) = &report.first_6_characters_of_sha3_256_hash_of_corresponding_file {
        // Convert byte array to string for validation
        let hash_string = std::str::from_utf8(hash).map_err(|_| OracleError::InvalidFileHashLength)?;
        if !hash_string.chars().all(|c| c.is_ascii_hexdigit()) {
            msg!("Error: Invalid File Hash (Non-hex characters)");
            return Err(OracleError::InvalidFileHashLength.into());
        }
    } else {
        return Err(OracleError::MissingFileHash.into());
    }
    Ok(())
}


impl OracleContractState {
    pub fn assess_and_apply_bans(&mut self, current_time: u64) {
        for contributor in &mut self.contributors {
            if contributor.calculate_is_banned(current_time) {
                // Replace banned contributors with a placeholder
                *contributor = Contributor::default();
            } else {
                contributor.handle_consensus_failure(current_time);
            }
        }
    }
}

impl Contributor {

    // Method to handle consensus failure
    pub fn handle_consensus_failure(&mut self, current_time: u64) {
        self.consensus_failures += 1;
        if self.total_reports_submitted <= CONTRIBUTIONS_FOR_TEMPORARY_BAN && self.consensus_failures % TEMPORARY_BAN_THRESHOLD == 0 {
            // Apply temporary ban
            self.ban_expiry = current_time + TEMPORARY_BAN_DURATION;
        } else if self.total_reports_submitted >= CONTRIBUTIONS_FOR_PERMANENT_BAN && self.consensus_failures >= PERMANENT_BAN_THRESHOLD {
            // Apply permanent ban
            self.ban_expiry = u64::MAX;
        }
        msg!("Contributor Ban Update: Address: {}, Ban Expiry: {}", self.reward_address, self.ban_expiry);
    }
    // Method to check if the contributor is recently active
    fn calculate_is_recently_active(&self, last_txid_request_time: u64) -> bool {
        // Define a threshold for recent activity (e.g., active within the last 24 hours)
        let recent_activity_threshold = 86_400; // seconds in 24 hours

        // Convert `last_active_timestamp` to i64 for comparison
        let last_active_timestamp_i64 = self.last_active_timestamp as i64;

        // Check if the contributor was active after the last request was made
        last_active_timestamp_i64 >= last_txid_request_time as i64 &&
            Clock::get().unwrap().unix_timestamp - last_active_timestamp_i64 < recent_activity_threshold as i64
    }
    // Method to check if the contributor is reliable
    fn calculate_is_reliable(&self) -> bool {
        // Define what makes a contributor reliable; for example, a high reliability score which is a ratio of accurate reports to total reports
        if self.total_reports_submitted == 0 {
            return false; // Avoid division by zero
        }
        let reliability_ratio = self.accurate_reports_count as f32 / self.total_reports_submitted as f32;
        reliability_ratio >= 0.8 // Example threshold for reliability, e.g., 80% accuracy
    }    
    // Check if the contributor is currently banned
    pub fn calculate_is_banned(&self, current_time: u64) -> bool {
        current_time < self.ban_expiry
    }
    // Method to determine if the contributor is eligible for rewards
    pub fn calculate_is_eligible_for_rewards(&self) -> bool {
        self.total_reports_submitted >= MIN_REPORTS_FOR_REWARD 
            && self.reliability_score >= 80 
            && self.compliance_score >= MIN_COMPLIANCE_SCORE_FOR_REWARD
    }

}


#[derive(Accounts)]
pub struct SetBridgeContract<'info> {
    #[account(mut, has_one = admin_pubkey)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    pub admin_pubkey: Signer<'info>,
}

impl<'info> SetBridgeContract<'info> {
    pub fn set_bridge_contract(ctx: Context<SetBridgeContract>, bridge_contract_pubkey: Pubkey) -> Result<()> {
        let state = &mut ctx.accounts.oracle_contract_state;
        state.bridge_contract_pubkey = bridge_contract_pubkey;
        msg!("Bridge contract pubkey updated: {:?}", bridge_contract_pubkey);
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(txid: String)] // Include txid as part of the instruction
pub struct ProcessPayment<'info> {
    /// CHECK: This is checked in the handler function to verify it's the bridge contract.
    #[account(signer)]
    pub source_account: AccountInfo<'info>,

    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    #[account(
        mut,
        seeds = [create_seed("pending_payment", &txid, &source_account.key()).as_ref()],
        bump // You won't explicitly include the bump here; it's handled by Anchor
    )]
    pub pending_payment_account: Account<'info, PendingPaymentAccount>,

    pub system_program: Program<'info, System>,
}


pub fn process_payment_helper(
    ctx: Context<ProcessPayment>, 
    txid: [u8; MAX_TXID_LENGTH], // 64 bytes for txid, 
    amount: u64
) -> Result<()> {
    // Access the pending payment account using the txid as a seed
    let pending_payment_account = &mut ctx.accounts.pending_payment_account;

    // Ensure the payment corresponds to the provided txid
    if pending_payment_account.pending_payment.txid != txid {
        return Err(OracleError::PaymentNotFound.into());
    }

    // Verify the payment amount matches the expected amount
    if pending_payment_account.pending_payment.expected_amount != amount {
        return Err(OracleError::InvalidPaymentAmount.into());
    }

    // Mark the payment as received
    pending_payment_account.pending_payment.payment_status = PaymentStatus::Received;

    Ok(())
}


#[derive(Accounts)]
pub struct WithdrawFunds<'info> {
    #[account(
        mut,
        constraint = oracle_contract_state.admin_pubkey == *admin_account.key @ OracleError::UnauthorizedWithdrawalAccount,
    )]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    /// CHECK: The admin_account is manually verified in the instruction to ensure it's the correct and authorized account for withdrawal operations. This includes checking if the account matches the admin_pubkey stored in oracle_contract_state.
    pub admin_account: AccountInfo<'info>,

    #[account(mut)]
    pub reward_pool_account: Account<'info, RewardPool>,
    #[account(mut)]
    pub fee_receiving_contract_account: Account<'info, FeeReceivingContract>,
    pub system_program: Program<'info, System>,
}

impl<'info> WithdrawFunds<'info> {
    pub fn execute(ctx: Context<WithdrawFunds>, reward_pool_amount: u64, fee_receiving_amount: u64) -> Result<()> {
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
        **reward_pool_account.to_account_info().lamports.borrow_mut() -= reward_pool_amount;
        **admin_account.lamports.borrow_mut() += reward_pool_amount;

        // Transfer SOL from the fee receiving contract account to the admin account
        if **fee_receiving_contract_account.to_account_info().lamports.borrow() < fee_receiving_amount {
            return Err(OracleError::InsufficientFunds.into());
        }
        **fee_receiving_contract_account.to_account_info().lamports.borrow_mut() -= fee_receiving_amount;
        **admin_account.lamports.borrow_mut() += fee_receiving_amount;

        msg!("Withdrawal successful: {} lamports transferred from reward pool and {} lamports from fee receiving contract to admin account", reward_pool_amount, fee_receiving_amount);
        Ok(())
    }
}


declare_id!("AfP1c4sFcY1FeiGjQEtyxCim8BRnw22okNbKAsH2sBsB");

#[program]
pub mod solana_pastel_oracle_program {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, admin_pubkey: Pubkey) -> Result<()> {
        msg!("Initializing Oracle Contract State");
        ctx.accounts.initialize_oracle_state(admin_pubkey)?;
        msg!("Oracle Contract State Initialized with Admin Pubkey: {:?}", admin_pubkey);
    
        // Logging for Reward Pool and Fee Receiving Contract Accounts
        msg!("Reward Pool Account: {:?}", ctx.accounts.reward_pool_account.key());
        msg!("Fee Receiving Contract Account: {:?}", ctx.accounts.fee_receiving_contract_account.key());
    
        Ok(())
    }
    
    pub fn reallocate_oracle_state(ctx: Context<ReallocateOracleState>) -> Result<()> {
        ReallocateOracleState::execute(ctx)
    }

    pub fn register_new_data_contributor(ctx: Context<RegisterNewDataContributor>) -> Result<()> {
        register_new_data_contributor_helper(ctx)
    }

    pub fn add_txid_for_monitoring(ctx: Context<AddTxidForMonitoring>, data: AddTxidForMonitoringData) -> Result<()> {
        add_txid_for_monitoring_helper(ctx, data)
    }

    pub fn add_pending_payment(ctx: Context<HandlePendingPayment>, txid: String, expected_amount_str: String, payment_status_str: String) -> Result<()> {
        let expected_amount = expected_amount_str.parse::<u64>()
            .map_err(|_| OracleError::PendingPaymentInvalidAmount)?;
    
        // Convert the payment status from string to enum
        let payment_status = match payment_status_str.as_str() {
            "Pending" => PaymentStatus::Pending,
            "Received" => PaymentStatus::Received,
            _ => return Err(OracleError::InvalidPaymentStatus.into()),
        };
    
        // Convert txid string to byte array
        let txid_bytes = PastelTxStatusReport::string_to_fixed_array(&txid);

        let pending_payment = PendingPayment {
            txid: txid_bytes,
            expected_amount,
            payment_status,
        };

        add_pending_payment_helper(ctx, txid_bytes, pending_payment)
            .map_err(|e| e.into())
    }
    
    
    pub fn process_payment(ctx: Context<ProcessPayment>, txid: String, amount: u64) -> Result<()> {
        // Convert the txid String to a [u8; MAX_TXID_LENGTH] array using the existing utility function
        let txid_array = PastelTxStatusReport::string_to_fixed_array(&txid);
    
        // Call the internal helper function with the converted txid
        process_payment_helper(ctx, txid_array, amount)
    }

    pub fn submit_data_report(
        ctx: Context<SubmitDataReport>, 
        txid: String, 
        txid_status_str: String, 
        pastel_ticket_type_str: String, 
        first_6_characters_hash: String, 
        contributor_reward_address: Pubkey
    ) -> ProgramResult {
        msg!("In `submit_data_report` function -- Params: txid={}, txid_status_str={}, pastel_ticket_type_str={}, first_6_chars_hash={}, contributor_addr={}",
            txid, txid_status_str, pastel_ticket_type_str, first_6_characters_hash, contributor_reward_address);
    
        // Convert the txid_status from string to enum
        let txid_status = match txid_status_str.as_str() {
            "Invalid" => TxidStatus::Invalid,
            "PendingMining" => TxidStatus::PendingMining,
            "MinedPendingActivation" => TxidStatus::MinedPendingActivation,
            "MinedActivated" => TxidStatus::MinedActivated,
            _ => return Err(ProgramError::from(OracleError::InvalidTxidStatus))
        };
    
        // Convert the pastel_ticket_type from string to enum
        let pastel_ticket_type = match pastel_ticket_type_str.as_str() {
            "Sense" => PastelTicketType::Sense,
            "Cascade" => PastelTicketType::Cascade,
            "Nft" => PastelTicketType::Nft,
            "InferenceApi" => PastelTicketType::InferenceApi,
            _ => return Err(ProgramError::from(OracleError::InvalidPastelTicketType))
        };
    
        // Fetch current timestamp from Solana's clock
        let timestamp = Clock::get()?.unix_timestamp as u64;

        // Convert txid and first_6_characters_hash from String to fixed-size byte arrays
        let txid_bytes = PastelTxStatusReport::string_to_fixed_array(&txid);
        let first_6_characters_hash_bytes = PastelTxStatusReport::string_to_fixed_array_6(&first_6_characters_hash);

        // Construct the report
        let report = PastelTxStatusReport {
            txid: txid_bytes,
            txid_status,
            pastel_ticket_type: Some(pastel_ticket_type),
            first_6_characters_of_sha3_256_hash_of_corresponding_file: Some(first_6_characters_hash_bytes),
            timestamp,
            contributor_reward_address,
        };

        // Call the helper function to submit the report
        submit_data_report_helper(ctx, txid_bytes, report, contributor_reward_address)    
    }
    
    pub fn request_reward(ctx: Context<RequestReward>, contributor_address: Pubkey) -> Result<()> {
        request_reward_helper(ctx, contributor_address)
    }

    pub fn set_bridge_contract(ctx: Context<SetBridgeContract>, bridge_contract_pubkey: Pubkey) -> Result<()> {
        SetBridgeContract::set_bridge_contract(ctx, bridge_contract_pubkey)
    }

    pub fn withdraw_funds(ctx: Context<WithdrawFunds>, reward_pool_amount: u64, fee_receiving_amount: u64) -> Result<()> {
        WithdrawFunds::execute(ctx, reward_pool_amount, fee_receiving_amount)
    }

}