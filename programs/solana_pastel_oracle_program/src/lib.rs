use anchor_lang::prelude::*;
use std::collections::HashMap;
use std::hash::Hash;
use std::cmp;

const REGISTRATION_ENTRANCE_FEE_SOL: u64 = 10_000_000; // 0.10 SOL in lamports
const MIN_REPORTS_FOR_REWARD: u32 = 100; // Data Contributor must submit at least 100 reports to be eligible for rewards
const MIN_COMPLIANCE_SCORE_FOR_REWARD: i32 = 80; // Data Contributor must have a compliance score of at least 80 to be eligible for rewards
const BASE_REWARD_AMOUNT_SOL: u64 = 100_000; // 0.0001 SOL in lamports is the base reward amount, which is scaled based on the number of highly reliable contributors
const COST_IN_SOL_OF_ADDING_PASTEL_TXID_FOR_MONITORING: u64 = 100_000; // 0.0001 SOL in lamports
const PERMANENT_BAN_THRESHOLD: u32 = 100; // Number of non-consensus report submissions for permanent ban
const CONTRIBUTIONS_FOR_PERMANENT_BAN: u32 = 250; // Considered for permanent ban after 250 contributions
const TEMPORARY_BAN_THRESHOLD: u32 = 5; // Number of non-consensus report submissions for temporary ban
const CONTRIBUTIONS_FOR_TEMPORARY_BAN: u32 = 50; // Considered for temporary ban after 50 contributions
const TEMPORARY_BAN_DURATION: u64 = 604800; // Duration of temporary ban in seconds (e.g., 1 week)
const MIN_NUMBER_OF_ORACLES: usize = 3; // Minimum number of oracles to calculate consensus
const MAX_SUBMITTED_RESPONSES_PER_TXID: usize = 100; // Maximum number of responses that can be submitted corresponding to a single txid
const MAX_STORED_REPORTS: usize = 250; // Maximum number of reports to store before cleanup
const REPORT_RETENTION_PERIOD: u64 = 10_800; // Retention period in seconds (i.e., 3 hours)
const SUBMISSION_COUNT_RETENTION_PERIOD: u64 = 86_400; // Number of seconds to retain submission counts (i.e., 24 hours)

#[error_code]
pub enum OracleError {
    ContributorAlreadyRegistered,
    UnregisteredOracle,
    EmptyResponseData,
    ReportDataWrongSize,
    InvalidResponseTimestamp,
    InvalidResponseDataFormat,
    InvalidTxid,
    InvalidTxidStatus,
    InvalidPastelTicketType,
    InvalidFileHashLength,
    SubmissionCountOverflow,
    SubmissionCountExceeded,
    MissingPastelTicketType,
    MissingFileHash,
    InvalidTimestamp,
    RegistrationFeeNotPaid,
    PaymentNotVerified,
    TxidNotRegistered,
    RewardTransferFailed,
    NotEligibleForReward,
    InvalidAccountData,
    InsufficientFunds,
    Unauthorized
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, AnchorSerialize, AnchorDeserialize)]
pub enum TxidStatus {
    Invalid,
    PendingMining,
    MinedPendingActivation,
    MinedActivated,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, AnchorSerialize, AnchorDeserialize)]
pub enum PastelTicketType {
    Sense,
    Cascade,
    Nft,
    InferenceApi,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, AnchorSerialize, AnchorDeserialize)]
pub struct PastelTxStatusReport {
    pub txid: String,
    pub txid_status: TxidStatus,
    pub pastel_ticket_type: Option<PastelTicketType>,
    pub first_6_characters_of_sha3_256_hash_of_corresponding_file: Option<String>,
    pub timestamp: u64,
    pub contributor_reward_address: Pubkey,
}

#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct TxidSubmissionCount {
    pub txid: String,
    pub count: u32,
    pub last_updated: u64,
}

#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct ConsensusSummary {
    pub txid: String,
    pub consensus_status_result: TxidStatus,
    pub consensus_first_6_characters_of_sha3_256_hash_of_corresponding_file: String,
    pub contributing_reports_count: u32,
}

#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct PendingPayment {
    pub txid: String,
    pub expected_amount: u64,
    pub payment_status: PaymentStatus,
}

#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub enum PaymentStatus {
    Pending,
    Received,
}

#[account]
pub struct RewardPool {
    // Since this account is only used for holding and transferring SOL, no fields are necessary.
}

#[account]
pub struct FeeReceivingContract {
    // Since this account is only used for holding and transferring SOL, no fields are necessary.
}

#[derive(Accounts)]
#[instruction(admin_pubkey: Pubkey)]
pub struct Initialize<'info> {
    #[account(init, payer = user, space = 8 + 10240)] // Adjust the space as needed
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

#[account]
pub struct Contributor {
    pub reward_address: Pubkey,
    pub registration_entrance_fee_transaction_signature: String,
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

#[account]
pub struct OracleContractState {
    pub is_initialized: bool,
    pub admin_pubkey: Pubkey,
    pub contributors: Vec<Contributor>,
    pub txid_submission_counts: Vec<TxidSubmissionCount>,
    pub monitored_txids: Vec<String>,
    pub reports: HashMap<String, PastelTxStatusReport>,
    pub consensus_summaries: HashMap<String, ConsensusSummary>,
    pub pending_payments: HashMap<String, PendingPayment>,
    pub reward_pool_account: Pubkey,
    pub reward_pool_nonce: u8,
    pub fee_receiving_contract_account: Pubkey,
    pub fee_receiving_contract_nonce: u8,
    pub bridge_contract_pubkey: Pubkey,
}


#[derive(Accounts)]
pub struct UpdateActivityAndCompliance<'info> {
    #[account(mut)]
    pub contributor: Account<'info, Contributor>,
}


#[derive(Accounts)]
pub struct RequestReward<'info> {
    #[account(mut)]
    pub reward_pool_account: Account<'info, RewardPool>,
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    #[account(mut)]
    pub contributor: Account<'info, Contributor>,
    pub system_program: Program<'info, System>,
}

pub fn request_reward_helper(ctx: Context<RequestReward>) -> Result<()> {
    let contributor = &mut ctx.accounts.contributor;

    // Convert i64 Unix timestamp to u64
    let current_unix_timestamp = Clock::get()?.unix_timestamp as u64;

    // Store eligibility and ban status in temporary variables
    let is_eligible_for_rewards = contributor.is_eligible_for_rewards;
    let is_banned = contributor.is_banned(current_unix_timestamp);

    // Check if the contributor is eligible for a reward
    if is_eligible_for_rewards && !is_banned {
        // Calculate the reward amount for the contributor
        let reward_amount = BASE_REWARD_AMOUNT_SOL; // Adjust based on your logic

        // Transfer the reward from the reward pool to the contributor
        **contributor.to_account_info().lamports.borrow_mut() += reward_amount;
        **ctx.accounts.reward_pool_account.to_account_info().lamports.borrow_mut() -= reward_amount;

        // Update state to reflect the reward distribution
        let contributor_address_str = contributor.reward_address.to_string();
        let state = &mut ctx.accounts.oracle_contract_state;
        state.pending_payments.remove(&contributor_address_str);
    } else {
        return Err(OracleError::NotEligibleForReward.into());
    }

    Ok(())
}

#[derive(Accounts)]
pub struct RegisterNewDataContributor<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    #[account(mut, signer)]
    pub contributor_account: AccountInfo<'info>,
    #[account(mut)]
    pub reward_pool_account: Account<'info, RewardPool>,
    #[account(mut)]
    pub fee_receiving_contract_account: Account<'info, FeeReceivingContract>,
}


pub fn register_new_data_contributor_helper(ctx: Context<RegisterNewDataContributor>) -> Result<()> {
    let state = &mut ctx.accounts.oracle_contract_state;

    // Check if the contributor is already registered
    if state.contributors.iter().any(|c| c.reward_address == *ctx.accounts.contributor_account.key) {
        return Err(OracleError::ContributorAlreadyRegistered.into());
    }

    // Check if the fee_receiving_contract_account received the registration fee
    if ctx.accounts.fee_receiving_contract_account.to_account_info().lamports() < REGISTRATION_ENTRANCE_FEE_SOL {
        return Err(OracleError::RegistrationFeeNotPaid.into());
    }

    // Transfer the fee to the reward pool account
    **ctx.accounts.reward_pool_account.to_account_info().lamports.borrow_mut() += ctx.accounts.fee_receiving_contract_account.to_account_info().lamports();
    **ctx.accounts.fee_receiving_contract_account.to_account_info().lamports.borrow_mut() = 0;

    // Create and add the new contributor
    let new_contributor = Contributor {
        reward_address: *ctx.accounts.contributor_account.key,
        registration_entrance_fee_transaction_signature: String::new(), // Replace with actual data if available
        compliance_score: 0, // Initial compliance score
        last_active_timestamp: Clock::get()?.unix_timestamp as u64, // Set the last active timestamp to the current time
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

    // Append the new contributor to the state
    state.contributors.push(new_contributor);

    // Logging for debug purposes
    msg!("New Contributor Registered: {:?}", ctx.accounts.contributor_account.key);

    Ok(())
}


// This method will now be part of an instruction or a helper function.
pub fn update_activity_and_compliance_helper(
    contributor: &mut Contributor, 
    current_timestamp: u64, 
    is_accurate: bool,
) -> Result<()> {
    // Ensure that you're correctly mutating fields in the contributor account
    contributor.last_active_timestamp = current_timestamp;
    contributor.total_reports_submitted += 1;

    let progressive_scaling = 1.0 / (1.0 + (contributor.total_reports_submitted as f32 * 0.01).log10());
    let time_diff = current_timestamp - contributor.last_active_timestamp;
    let days_inactive = time_diff as f32 / 86_400.0; // 86,400 seconds in a day
    let time_weight = (2.0_f32).powf(-days_inactive / 30.0); // Half-life of 30 days

    let score_increment = 10 - (contributor.accurate_reports_count as i32 / 10).min(5);
    let score_decrement = 5 + (contributor.total_reports_submitted as i32 - contributor.accurate_reports_count as i32 / 10).min(5);
    let streak_bonus = cmp::min(contributor.current_streak / 10, 5) as f32;

    if is_accurate {
        contributor.accurate_reports_count += 1;
        contributor.current_streak += 1;
        contributor.compliance_score += ((score_increment as f32) * progressive_scaling * time_weight + streak_bonus) as i32;
    } else {
        contributor.current_streak = 0;
        contributor.compliance_score -= (score_decrement as f32 * time_weight) as i32;
    }

    contributor.compliance_score = cmp::min(cmp::max(contributor.compliance_score, -100), 100);

    // Calculate reliability score
    if contributor.total_reports_submitted > 0 {
        contributor.reliability_score = (contributor.accurate_reports_count as f32 / contributor.total_reports_submitted as f32 * 100.0) as u32;
    }

    // Handle consensus failure and bans
    contributor.consensus_failures += 1;
    if contributor.total_reports_submitted <= CONTRIBUTIONS_FOR_TEMPORARY_BAN && contributor.consensus_failures % TEMPORARY_BAN_THRESHOLD == 0 {
        contributor.ban_expiry = current_timestamp + TEMPORARY_BAN_DURATION;
    } else if contributor.total_reports_submitted >= CONTRIBUTIONS_FOR_PERMANENT_BAN && contributor.consensus_failures >= PERMANENT_BAN_THRESHOLD {
        contributor.ban_expiry = u64::MAX;
    }

    // Update statuses
    contributor.is_recently_active = current_timestamp - contributor.last_active_timestamp < 86_400;
    contributor.is_reliable = contributor.total_reports_submitted != 0 && (contributor.accurate_reports_count as f32 / contributor.total_reports_submitted as f32) >= 0.8;
    contributor.is_eligible_for_rewards = contributor.total_reports_submitted >= MIN_REPORTS_FOR_REWARD && contributor.is_reliable;

    Ok(())

}


// Anchor instruction
pub fn update_activity_and_compliance(
    ctx: Context<UpdateActivityAndCompliance>, 
    current_timestamp: u64, 
    is_accurate: bool
) -> Result<()> {
    let contributor = &mut ctx.accounts.contributor;
    update_activity_and_compliance_helper(contributor, current_timestamp, is_accurate)
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct AddTxidForMonitoringData {
    pub txid: String,
}

#[derive(Accounts)]
pub struct AddTxidForMonitoring<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    #[account(signer)]
    pub caller: AccountInfo<'info>,
}

pub fn add_txid_for_monitoring_helper(
    ctx: Context<AddTxidForMonitoring>, 
    data: AddTxidForMonitoringData
) -> Result<()> {
    let state = &mut ctx.accounts.oracle_contract_state;

    // Clone the txid at the beginning to avoid ownership issues
    let txid = data.txid.clone();

    // Verify that the caller is the bridge contract
    if ctx.accounts.caller.key != &state.bridge_contract_pubkey {
        return Err(OracleError::InvalidAccountData.into());
    }

    // Add the TXID to the monitored list and pending payments
    state.monitored_txids.push(txid.clone());
    state.pending_payments.insert(
        txid.clone(),
        PendingPayment {
            txid,
            expected_amount: COST_IN_SOL_OF_ADDING_PASTEL_TXID_FOR_MONITORING,
            payment_status: PaymentStatus::Pending,
        },
    );

    msg!("New TXID for Monitoring: {}", data.txid);
    Ok(())
}

impl<'info> Initialize<'info> {
    pub fn initialize_oracle_state(&mut self, admin_pubkey: Pubkey) -> Result<()> {
        let state = &mut self.oracle_contract_state;
        // Initialize the OracleContractState
        state.is_initialized = true;
        state.admin_pubkey = admin_pubkey;
        state.contributors = Vec::new();
        state.txid_submission_counts = Vec::new();
        state.monitored_txids = Vec::new();
        state.reports = HashMap::new();
        state.consensus_summaries = HashMap::new();
        state.pending_payments = HashMap::new();
        state.bridge_contract_pubkey = Pubkey::default(); // Initialize with default or actual value
        Ok(())
    }
}


#[derive(Accounts)]
pub struct ProcessPastelTxStatusReport<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    #[account(mut, signer)]
    pub contributor: AccountInfo<'info>,
    // You can add other accounts as needed
}

#[derive(Accounts)]
pub struct SubmitDataReport<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    #[account(mut, signer)]
    pub contributor: AccountInfo<'info>,
    // Add other necessary accounts
}

fn should_calculate_consensus(state: &OracleContractState, txid: &str) -> Result<bool> {
    
    // Retrieve the count of submissions and last updated timestamp for the given txid
    let (submission_count, last_updated) = state.txid_submission_counts.iter()
        .find(|c| c.txid == *txid)
        .map(|c| (c.count, c.last_updated))
        .unwrap_or((0, 0));
    // Calculate the aspirational minimum target based on reliable and active contributors
    let active_reliable_contributors = state.contributors.iter()
        .filter(|c| c.is_recently_active(last_updated) && c.is_reliable())
        .count();
    // Define the minimum number of reports required before attempting to calculate consensus
    let min_reports_for_consensus = std::cmp::max(MIN_NUMBER_OF_ORACLES, active_reliable_contributors);
    // Check if the received reports have reached the minimum threshold
    Ok(submission_count >= min_reports_for_consensus as u32)
}

fn cleanup_old_submission_counts(state: &mut OracleContractState) -> Result<()> {
    let current_time = Clock::get()?.unix_timestamp as u64;
    state.txid_submission_counts.retain(|count| {
        current_time - count.last_updated < SUBMISSION_COUNT_RETENTION_PERIOD
    });
    Ok(())
}

pub fn submit_data_report_helper(ctx: Context<SubmitDataReport>, report: PastelTxStatusReport) -> Result<()> {
    let contributor_key = ctx.accounts.contributor.key();
    let state = &mut ctx.accounts.oracle_contract_state;

    // Check if the contributor is registered and not banned
    let contributor = state.contributors.iter().find(|c| c.reward_address == contributor_key);
    let current_timestamp = Clock::get()?.unix_timestamp.try_into().map_err(|_| OracleError::InvalidTimestamp)?;
    if contributor.is_none() || contributor.unwrap().is_banned(current_timestamp) {
        return Err(OracleError::UnregisteredOracle.into());
    }

    let state = &mut ctx.accounts.oracle_contract_state;

    // Validate the report
    validate_data_contributor_report(&report)?;

    // Add or update the report in the OracleContractState
    state.add_or_update_report(report.clone(), Clock::get()?.unix_timestamp as u64)?;

    // Check if consensus should be calculated
    if should_calculate_consensus(state, &report.txid)? {
        calculate_consensus_and_cleanup(state, &report.txid)?;
    }

    // Cleanup old submission counts
    cleanup_old_submission_counts(state)?;

    Ok(())
}

// Normalize the compliance score to a positive range, e.g., [0, 100]
fn normalize_compliance_score(score: i32) -> i32 {
    let max_score = 100;
    let min_score = -100;
    // Adjust score to be in the range [0, 200]
    let adjusted_score = score - min_score;
    // Scale down to be in the range [0, 100]
    (adjusted_score * max_score) / (max_score - min_score)
}

fn calculate_consensus(
    reports: &HashMap<String, PastelTxStatusReport>,
    contributors: &Vec<Contributor>,
    txid: &str
) -> (TxidStatus, String) {
    let mut weighted_status_counts = HashMap::new();
    let mut weighted_hash_counts = HashMap::new();

    for report in reports.values() {
        if report.txid == *txid {
            if let Some(contributor) = contributors.iter().find(|c| c.reward_address == report.contributor_reward_address) {
                let weight = normalize_compliance_score(contributor.compliance_score);
                *weighted_status_counts.entry(report.txid_status).or_insert(0) += weight;
                if let Some(hash) = &report.first_6_characters_of_sha3_256_hash_of_corresponding_file {
                    *weighted_hash_counts.entry(hash.clone()).or_insert(0) += weight;
                }
            }
        }
    }

    let consensus_status = weighted_status_counts.iter()
        .max_by_key(|&(_, &count)| count)
        .map(|(&status, _)| status)
        .unwrap_or(TxidStatus::Invalid);

    let consensus_hash = weighted_hash_counts.iter()
        .max_by_key(|&(_, &count)| count)
        .map(|(hash, _)| hash.clone())
        .unwrap_or_default();

    (consensus_status, consensus_hash)
}


fn calculate_consensus_and_cleanup(state: &mut OracleContractState, txid: &str) -> Result<()> {
    // Calculate consensus
    let (consensus_status_result, consensus_hash) = calculate_consensus(&state.reports, &state.contributors, txid);

    // Iterate over reports and update contributors
    for report in state.reports.values() {
        if report.txid == *txid {
            if let Some(contributor) = state.contributors.iter_mut().find(|c| c.reward_address == report.contributor_reward_address) {
                let is_accurate_status = report.txid_status == consensus_status_result;
                let is_accurate_hash = report.first_6_characters_of_sha3_256_hash_of_corresponding_file.as_ref().map_or(false, |hash| hash == &consensus_hash);
                
                let is_accurate = is_accurate_status && is_accurate_hash;

                // Update contributor's activity and compliance
                update_activity_and_compliance_helper(contributor, Clock::get()?.unix_timestamp as u64, is_accurate)?;
            }
        }
    }

    // Cleanup logic for old reports
    state.reports.retain(|k, _| k != txid);

    Ok(())
}


// Function to handle the submission of Pastel transaction status reports
fn validate_data_contributor_report(report: &PastelTxStatusReport) -> Result<()> {
    // Validate the TXID is non-empty
    if report.txid.trim().is_empty() {
        return Err(OracleError::InvalidTxid.into());
    }
    // Validate TXID status
    match report.txid_status {
        TxidStatus::MinedActivated | TxidStatus::MinedPendingActivation | TxidStatus::PendingMining | TxidStatus::Invalid => {},
    }
    
    // Validate the Pastel ticket type is present and valid
    if report.pastel_ticket_type.is_none() {
        return Err(OracleError::MissingPastelTicketType.into());
    }
    // Validate the SHA3-256 hash of the corresponding file
    if let Some(hash) = &report.first_6_characters_of_sha3_256_hash_of_corresponding_file {
        if hash.len() != 6 || !hash.chars().all(|c| c.is_digit(16)) {
            return Err(OracleError::InvalidFileHashLength.into());
        }
    } else {
        return Err(OracleError::MissingFileHash.into());
    }
    // Validate the timestamp
    let current_timestamp = Clock::get()?.unix_timestamp as u64;
    if report.timestamp > current_timestamp {
        return Err(OracleError::InvalidTimestamp.into());
    }
    
    Ok(())
}


impl OracleContractState {

    pub fn get_eligible_contributors(&self) -> Vec<Pubkey> {
        self.contributors
            .iter()
            .filter(|contributor| contributor.is_eligible_for_rewards)
            .map(|contributor| contributor.reward_address)
            .collect()
    }

    pub fn add_or_update_report(&mut self, report: PastelTxStatusReport, current_time: u64) -> Result<()> {
        let txid_submission_count = self.txid_submission_counts
            .iter_mut()
            .find(|c| c.txid == report.txid);

        if let Some(count) = txid_submission_count {
            if count.count >= MAX_SUBMITTED_RESPONSES_PER_TXID as u32 {
                return Err(OracleError::SubmissionCountExceeded.into());
            }
            count.count += 1;
            count.last_updated = current_time;
        } else {
            self.txid_submission_counts.push(TxidSubmissionCount {
                txid: report.txid.clone(),
                count: 1,
                last_updated: current_time,
            });
        }

        let report_with_timestamp = PastelTxStatusReport {
            timestamp: current_time,
            ..report
        };

        if self.reports.len() >= MAX_STORED_REPORTS {
            self.cleanup_old_reports(current_time);
        }

        self.reports.insert(report_with_timestamp.txid.clone(), report_with_timestamp);
        Ok(())
    }

    pub fn cleanup_old_reports(&mut self, current_time: u64) {
        self.reports.retain(|_, report| {
            current_time - report.timestamp < REPORT_RETENTION_PERIOD
        });
    }

    pub fn add_consensus_summary(&mut self, summary: ConsensusSummary) {
        self.consensus_summaries.insert(summary.txid.clone(), summary);
    }    

    pub fn get_highly_reliable_contributors(&self) -> Vec<Contributor> {
        self.contributors.iter()
            .filter(|c| c.reliability_score > 80)
            .cloned()
            .collect()
    }


    pub fn remove_underperforming_contributor(&mut self, reward_address: Pubkey) {
        self.contributors.retain(|c| c.reward_address != reward_address);
    }        

    pub fn is_contributor_banned(&self, reward_address: &Pubkey, current_time: u64) -> bool {
        self.contributors.iter().any(|c| 
            &c.reward_address == reward_address && c.ban_expiry > current_time
        )
    }

    pub fn assess_and_apply_bans(&mut self, current_time: u64) {
        for contributor in &mut self.contributors {
            contributor.handle_consensus_failure(current_time);
        }

        self.contributors.retain(|c| !c.is_banned(current_time));
    }
}

impl Contributor {

    // Method to update the reliability score based on the contributor's performance
    pub fn update_reliability_score(&mut self) {
        // Calculate the reliability score based on accurate_reports_count and total_reports_submitted
        if self.total_reports_submitted > 0 {
            // Cast the calculation result to u32 since reliability_score is u32
            self.reliability_score = (self.accurate_reports_count as f32 / self.total_reports_submitted as f32 * 100.0) as u32;
        }
    }
    
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
    }

    // Method to check if the contributor is recently active
    fn is_recently_active(&self, last_txid_request_time: u64) -> bool {
        // Define a threshold for recent activity (e.g., active within the last 24 hours)
        let recent_activity_threshold = 86_400; // seconds in 24 hours

        // Convert `last_active_timestamp` to i64 for comparison
        let last_active_timestamp_i64 = self.last_active_timestamp as i64;

        // Check if the contributor was active after the last request was made
        last_active_timestamp_i64 >= last_txid_request_time as i64 &&
            Clock::get().unwrap().unix_timestamp - last_active_timestamp_i64 < recent_activity_threshold as i64
    }

    // Method to check if the contributor is reliable
    fn is_reliable(&self) -> bool {
        // Define what makes a contributor reliable
        // For example, a high reliability score which is a ratio of accurate reports to total reports
        if self.total_reports_submitted == 0 {
            return false; // Avoid division by zero
        }

        let reliability_ratio = self.accurate_reports_count as f32 / self.total_reports_submitted as f32;
        reliability_ratio >= 0.8 // Example threshold for reliability, e.g., 80% accuracy
    }    

    // Check if the contributor is currently banned
    pub fn is_banned(&self, current_time: u64) -> bool {
        current_time < self.ban_expiry
    }

    // Method to determine if the contributor is eligible for rewards
    pub fn is_eligible_for_rewards(&self) -> bool {
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
pub struct WithdrawFunds<'info> {
    #[account(
        mut,
        constraint = oracle_contract_state.admin_pubkey == *admin_account.key @ OracleError::Unauthorized
    )]
    pub oracle_contract_state: Account<'info, OracleContractState>,
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
            return Err(OracleError::Unauthorized.into()); // Check if the admin_account is a signer
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
        ctx.accounts.initialize_oracle_state(admin_pubkey)
    }

    pub fn register_new_data_contributor(ctx: Context<RegisterNewDataContributor>) -> Result<()> {
        register_new_data_contributor_helper(ctx)
    }

    pub fn add_txid_for_monitoring(ctx: Context<AddTxidForMonitoring>, data: AddTxidForMonitoringData) -> Result<()> {
        add_txid_for_monitoring_helper(ctx, data)
    }    

    pub fn submit_data_report(ctx: Context<SubmitDataReport>, report: PastelTxStatusReport) -> Result<()> {
        submit_data_report_helper(ctx, report)
    }

    pub fn request_reward(ctx: Context<RequestReward>) -> Result<()> {
        request_reward_helper(ctx)
    }

    pub fn set_bridge_contract(ctx: Context<SetBridgeContract>, bridge_contract_pubkey: Pubkey) -> Result<()> {
        SetBridgeContract::set_bridge_contract(ctx, bridge_contract_pubkey)
    }

    pub fn withdraw_funds(ctx: Context<WithdrawFunds>, reward_pool_amount: u64, fee_receiving_amount: u64) -> Result<()> {
        WithdrawFunds::execute(ctx, reward_pool_amount, fee_receiving_amount)
    }

}