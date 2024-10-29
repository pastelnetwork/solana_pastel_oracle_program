pub mod big_number;
pub mod fixed_exp;
pub mod fixed_giga;

use crate::fixed_giga::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::{hash, Hash};
use anchor_lang::solana_program::sysvar::clock::Clock;
use anchor_lang::solana_program::sysvar::rent::Rent;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_lang::solana_program::{program::invoke, system_instruction};

const REGISTRATION_ENTRANCE_FEE_IN_LAMPORTS: u64 = 10_000_000; // 0.10 SOL in lamports
const MIN_NUMBER_OF_ORACLES: usize = 8; // Minimum number of oracles to calculate consensus
const MIN_REPORTS_FOR_REWARD: u32 = 10; // Data Contributor must submit at least 10 reports to be eligible for rewards
const BASE_REWARD_AMOUNT_IN_LAMPORTS: u64 = 100_000; // 0.0001 SOL in lamports is the base reward amount, which is scaled based on the number of highly reliable contributors
const COST_IN_LAMPORTS_OF_ADDING_PASTEL_TXID_FOR_MONITORING: u64 = 100_000; // 0.0001 SOL in lamports
const PERMANENT_BAN_THRESHOLD: u32 = 100; // Number of non-consensus report submissions for permanent ban
const CONTRIBUTIONS_FOR_PERMANENT_BAN: u32 = 250; // Considered for permanent ban after 250 contributions
const TEMPORARY_BAN_THRESHOLD: u32 = 5; // Number of non-consensus report submissions for temporary ban
const CONTRIBUTIONS_FOR_TEMPORARY_BAN: u32 = 50; // Considered for temporary ban after 50 contributions
const TEMPORARY_BAN_DURATION: u32 = 24 * 60 * 60; // Duration of temporary ban in seconds (e.g., 1 day)
const MAX_DURATION_IN_SECONDS_FROM_LAST_REPORT_SUBMISSION_BEFORE_COMPUTING_CONSENSUS: u32 = 10 * 60; // Maximum duration in seconds from last report submission for a given TXID before computing consensus (e.g., 10 minutes)
const DATA_RETENTION_PERIOD: u32 = 24 * 60 * 60; // How long to keep data in the contract state (1 day)
const SUBMISSION_COUNT_RETENTION_PERIOD: u32 = 24 * 60 * 60; // Number of seconds to retain submission counts (i.e., 24 hours)
const TXID_STATUS_VARIANT_COUNT: usize = 4; // Manually define the number of variants in TxidStatus
const MAX_TXID_LENGTH: usize = 64; // Maximum length of a TXID

const MIN_COMPLIANCE_SCORE_FOR_REWARD: u64 = 65_000000000; // Data Contributor must have a compliance score of at least 80 to be eligible for rewards
const MIN_RELIABILITY_SCORE_FOR_REWARD: u64 = 80_000000000; // Minimum reliability score to be eligible for rewards
const BASE_SCORE_INCREMENT: u64 = 20_000000000; // 20.0
const RELIABILITY_RATIO_THRESHOLD: u64 = 800000000; // 0.8
const DECAY_RATE: u64 = 990000000; // 0.99
const MAX_STREAK_BONUS: u64 = 3_000000000; // 3.0
const MAX_COMPLIANCE_SCORE: u64 = 100_000000000; // 100.0
const MID_COMPLIANCE_SCORE: u64 = 50_000000000; // 50.0

const ONE_TENTH: u64 = 100000000; // 0.1
const FIVE_TENTH: u64 = 500000000; // 0.5

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
    ContributorNotRegistered,
    ContributorBanned,
    EnoughReportsSubmittedForTxid,
    MaxSizeExceeded,
    InvalidAccountName,
}

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
    pub timestamp: u32,
    pub contributor_reward_address: Pubkey,
}

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
    pub timestamp: u32,
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

#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct TxidSubmissionCount {
    pub txid: String,
    pub count: u32,
    pub last_updated: u32,
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub struct PendingPayment {
    pub txid: String,
    pub expected_amount: u64,
    pub payment_status: PaymentStatus,
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub enum PaymentStatus {
    Pending,
    Received,
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

    #[account(mut, seeds = [b"temp_tx_status_report"], bump)]
    pub temp_report_account: Account<'info, TempTxStatusReportAccount>,

    #[account(mut, seeds = [b"contributor_data"], bump)]
    pub contributor_data_account: Account<'info, ContributorDataAccount>,

    #[account(mut, seeds = [b"txid_submission_counts"], bump)]
    pub txid_submission_counts_account: Account<'info, TxidSubmissionCountsAccount>,

    #[account(mut, seeds = [b"aggregated_consensus_data"], bump)]
    pub aggregated_consensus_data_account: Account<'info, AggregatedConsensusDataAccount>,

    pub system_program: Program<'info, System>,
}

fn update_submission_count(
    txid_submission_counts_account: &mut Account<TxidSubmissionCountsAccount>,
    txid: &str,
) -> Result<()> {
    // Get the current timestamp
    let current_timestamp = Clock::get()?.unix_timestamp as u32;

    let counts = &mut txid_submission_counts_account.submission_counts;

    // Use binary search directly without sorting, assuming counts is always kept sorted
    match counts.binary_search_by(|c| c.txid.as_str().cmp(txid)) {
        Ok(index) => {
            // Found the txid, update the count
            let count = &mut counts[index];
            count.count += 1;
            count.last_updated = current_timestamp;
        }
        Err(index) => {
            // Not found, insert at the position to keep the vector sorted
            counts.insert(
                index,
                TxidSubmissionCount {
                    txid: txid.to_string(),
                    count: 1,
                    last_updated: current_timestamp,
                },
            );
        }
    }

    Ok(())
}

pub fn get_report_account_pda(
    program_id: &Pubkey,
    txid: &str,
    contributor_reward_address: &Pubkey,
) -> (Pubkey, u8) {
    let seed_hash = create_seed("pastel_tx_status_report", txid, contributor_reward_address);
    Pubkey::find_program_address(&[seed_hash.as_ref()], program_id)
}

fn get_aggregated_data<'a>(
    aggregated_data_account: &'a Account<AggregatedConsensusDataAccount>,
    txid: &str,
) -> Option<&'a AggregatedConsensusData> {
    let consensus_data = &aggregated_data_account.consensus_data;
    match consensus_data.binary_search_by(|d| d.txid.as_str().cmp(txid)) {
        Ok(index) => Some(&consensus_data[index]),
        Err(_) => None,
    }
}

fn compute_consensus(aggregated_data: &AggregatedConsensusData) -> (TxidStatus, String) {
    // Find the index of the maximum status weight
    let (max_status_index, _) = aggregated_data
        .status_weights
        .iter()
        .enumerate()
        .max_by_key(|&(_, &weight)| weight)
        .unwrap();

    let consensus_status = usize_to_txid_status(max_status_index).unwrap_or(TxidStatus::Invalid);

    // Find the hash with the maximum weight
    let consensus_hash = aggregated_data
        .hash_weights
        .iter()
        .max_by_key(|hash_weight| hash_weight.weight)
        .map(|hash_weight| hash_weight.hash.clone())
        .unwrap_or_default();

    (consensus_status, consensus_hash)
}

fn apply_bans(contributor: &mut Contributor, current_timestamp: u32, is_accurate: bool) {
    if is_accurate {
        return; // No need to apply bans if the report is accurate
    }

    let failures = contributor.consensus_failures;
    let total_submissions = contributor.total_reports_submitted;

    if total_submissions >= CONTRIBUTIONS_FOR_PERMANENT_BAN
        && failures >= PERMANENT_BAN_THRESHOLD
    {
        contributor.ban_expiry = u32::MAX;
        msg!(
            "Contributor: {} is permanently banned as of {}",
            contributor.reward_address,
            current_timestamp
        );
    } else if total_submissions >= CONTRIBUTIONS_FOR_TEMPORARY_BAN
        && failures % TEMPORARY_BAN_THRESHOLD == 0
    {
        contributor.ban_expiry = current_timestamp + TEMPORARY_BAN_DURATION;
        msg!(
            "Contributor: {} is temporarily banned until {}",
            contributor.reward_address,
            contributor.ban_expiry
        );
    }
}

fn update_scores(contributor: &mut Contributor, current_timestamp: u32, is_accurate: bool) {
    let time_diff = current_timestamp
        .saturating_sub(contributor.last_active_timestamp)
        .to_fixed_giga();
    let hours_inactive = time_diff.div_up(3600_000000000);
    let current_streak = contributor.current_streak as u64 * ONE_TENTH;

    // Dynamic scaling for accuracy
    let accuracy_scaling = if is_accurate {
        (ONE + current_streak).min(TWO) // Increasing bonus for consecutive accuracy
    } else {
        ONE
    };

    let time_weight = ONE.div_up(ONE + hours_inactive.div_up(480_000000000));

    // Adjusted base increment for a more gradual increase
    let score_increment = BASE_SCORE_INCREMENT
        .mul_down(accuracy_scaling)
        .mul_down(time_weight);

    // Exponential penalty for inaccuracies
    let score_decrement = BASE_SCORE_INCREMENT
        .mul_up(ONE + contributor.consensus_failures as u64 * FIVE_TENTH)
        .min(MAX_STREAK_BONUS);

    // Adjusted decay rate 0.99
    let decay_factor = DECAY_RATE.pow_up(hours_inactive.div_up(24_000000000));

    let streak_bonus = if is_accurate {
        current_streak.min(MAX_STREAK_BONUS) // Enhanced streak bonus
    } else {
        ZERO
    };

    if is_accurate {
        contributor.total_reports_submitted += 1;
        contributor.accurate_reports_count += 1;
        contributor.current_streak += 1;
        contributor.compliance_score += score_increment + streak_bonus;
    } else {
        contributor.total_reports_submitted += 1;
        contributor.current_streak = 0;
        contributor.consensus_failures += 1;
        contributor.compliance_score = contributor.compliance_score.saturating_sub(score_decrement);
    }

    contributor.compliance_score = contributor.compliance_score.mul_up(decay_factor);

    // Integrating reliability score into compliance score calculation
    let reliability_factor = contributor
        .accurate_reports_count
        .to_fixed_giga()
        .div_down(contributor.total_reports_submitted.to_fixed_giga())
        .clamp(ZERO, ONE);
    contributor.compliance_score = contributor
        .compliance_score
        .mul_down(reliability_factor)
        .min(MAX_COMPLIANCE_SCORE);

    contributor.compliance_score = logistic_scale(
        contributor.compliance_score,
        MAX_COMPLIANCE_SCORE,
        ONE_TENTH,
        MID_COMPLIANCE_SCORE,
    ); // Adjusted logistic scaling

    contributor.reliability_score = reliability_factor * 100;

    log_score_updates(contributor);
}

fn logistic_scale(score: u64, max_value: u64, steepness: u64, midpoint: u64) -> u64 {
    // max_value / (1 + e ^ (-steepness * (score - midpoint)))
    let denom = if score > midpoint {
        ONE + steepness.mul_down(score - midpoint).neg_exp_down()
    } else if score < midpoint {
        ONE + steepness.mul_down(midpoint - score).exp_down()
    } else {
        TWO
    };
    max_value.div_down(denom)
}

fn log_score_updates(contributor: &Contributor) {
    msg!(
        "Scores After Update: Address: {}, Compliance Score: {}, Reliability Score: {}",
        contributor.reward_address,
        contributor.compliance_score,
        contributor.reliability_score
    );
}

fn update_statuses(contributor: &mut Contributor, current_timestamp: u32) {
    let recent_activity_threshold = 86_400; // 24 hours in seconds
    contributor.is_recently_active =
        current_timestamp.saturating_sub(contributor.last_active_timestamp) < recent_activity_threshold;

    let reliability_ratio = if contributor.total_reports_submitted > 0 {
        (contributor.accurate_reports_count as u64 * ONE)
            / (contributor.total_reports_submitted as u64)
    } else {
        0
    };

    contributor.is_reliable = reliability_ratio >= RELIABILITY_RATIO_THRESHOLD;
    contributor.is_eligible_for_rewards = contributor.calculate_is_eligible_for_rewards();
}

fn update_contributor(contributor: &mut Contributor, current_timestamp: u32, is_accurate: bool) {
    // Skip update if the contributor is banned
    if contributor.calculate_is_banned(current_timestamp) {
        msg!(
            "Contributor is currently banned and cannot be updated: {}",
            contributor.reward_address
        );
        return;
    }

    // Update scores
    update_scores(contributor, current_timestamp, is_accurate);

    // Apply bans based on report accuracy
    apply_bans(contributor, current_timestamp, is_accurate);

    // Update contributor statuses
    update_statuses(contributor, current_timestamp);
}

fn calculate_consensus(
    aggregated_data_account: &Account<AggregatedConsensusDataAccount>,
    temp_report_account: &TempTxStatusReportAccount,
    contributor_data_account: &mut Account<ContributorDataAccount>,
    txid: &str,
) -> Result<()> {
    let current_timestamp = Clock::get()?.unix_timestamp as u32;
    let (consensus_status, consensus_hash) = get_aggregated_data(aggregated_data_account, txid)
        .map(|data| compute_consensus(data))
        .unwrap_or((TxidStatus::Invalid, String::new()));

    let mut updated_contributors = Vec::new();
    let mut contributor_count = 0;

    for temp_report in &temp_report_account.reports {
        let common_data = &temp_report_account.common_reports[temp_report.common_data_ref as usize];
        let specific_data = &temp_report.specific_data;

        if common_data.txid == txid
            && !updated_contributors.contains(&specific_data.contributor_reward_address)
        {
            let contributors = &mut contributor_data_account.contributors;

            // Use binary search directly without sorting
            match contributors.binary_search_by(|c| c.reward_address.cmp(&specific_data.contributor_reward_address)) {
                Ok(index) => {
                    let contributor = &mut contributors[index];
                    let is_accurate = common_data.txid_status == consensus_status
                        && common_data
                            .first_6_characters_of_sha3_256_hash_of_corresponding_file
                            .as_ref()
                            .map_or(false, |hash| hash == &consensus_hash);
                    update_contributor(contributor, current_timestamp, is_accurate);
                }
                Err(_) => {
                    // Contributor not found; this should not happen
                    msg!(
                        "Contributor not found: {}",
                        specific_data.contributor_reward_address
                    );
                    return Err(OracleError::ContributorNotRegistered.into());
                }
            }

            updated_contributors.push(specific_data.contributor_reward_address);
            contributor_count += 1;
        }
    }
    msg!("Consensus reached for TXID: {}, Status: {:?}, Hash: {}, Number of Contributors Included: {}", txid, consensus_status, consensus_hash, contributor_count);

    Ok(())
}

pub fn apply_permanent_bans(contributor_data_account: &mut Account<ContributorDataAccount>) {
    let initial_len = contributor_data_account.contributors.len();
    contributor_data_account
        .contributors
        .retain(|c| c.ban_expiry != u32::MAX);

    let removed_count = initial_len - contributor_data_account.contributors.len();
    if removed_count > 0 {
        msg!(
            "Removed {} permanently banned contributors. Total contributors now: {}",
            removed_count,
            contributor_data_account.contributors.len()
        );
    }
}

fn post_consensus_tasks(
    txid_submission_counts_account: &mut Account<TxidSubmissionCountsAccount>,
    aggregated_data_account: &mut Account<AggregatedConsensusDataAccount>,
    temp_report_account: &mut TempTxStatusReportAccount,
    contributor_data_account: &mut Account<ContributorDataAccount>,
    txid: &str,
) -> Result<()> {
    let current_timestamp = Clock::get()?.unix_timestamp as u32;

    apply_permanent_bans(contributor_data_account);

    msg!("Now cleaning up unneeded data in TempTxStatusReportAccount...");
    // Cleanup unneeded data in TempTxStatusReportAccount
    temp_report_account.reports.retain(|temp_report| {
        // Access the common data from the TempTxStatusReportAccount
        let common_data = &temp_report_account.common_reports[temp_report.common_data_ref as usize];
        let specific_data = &temp_report.specific_data;
        common_data.txid != txid
            && current_timestamp - specific_data.timestamp < DATA_RETENTION_PERIOD
    });

    msg!("Now cleaning up unneeded data in AggregatedConsensusDataAccount...");
    // Cleanup unneeded data in AggregatedConsensusDataAccount
    aggregated_data_account
        .consensus_data
        .retain(|data| current_timestamp - data.last_updated < DATA_RETENTION_PERIOD);

    msg!("Now cleaning up unneeded data in TxidSubmissionCountsAccount...");
    // Cleanup old submission counts in TxidSubmissionCountsAccount
    txid_submission_counts_account
        .submission_counts
        .retain(|count| current_timestamp - count.last_updated < SUBMISSION_COUNT_RETENTION_PERIOD);

    msg!("Done with post-consensus tasks!");
    Ok(())
}

fn aggregate_consensus_data(
    aggregated_data_account: &mut Account<AggregatedConsensusDataAccount>,
    report: &PastelTxStatusReport,
    weight: u64,
    txid: &str,
) -> Result<()> {
    let scaled_weight = weight * 100; // Scaling by a factor of 100
    let current_timestamp = Clock::get()?.unix_timestamp as u32;

    let consensus_data = &mut aggregated_data_account.consensus_data;

    // Use binary search directly without sorting
    match consensus_data.binary_search_by(|d| d.txid.as_str().cmp(txid)) {
        Ok(index) => {
            // Update existing data
            let data_entry = &mut consensus_data[index];
            data_entry.status_weights[report.txid_status as usize] += scaled_weight;
            if let Some(hash) = &report.first_6_characters_of_sha3_256_hash_of_corresponding_file {
                update_hash_weight(&mut data_entry.hash_weights, hash, scaled_weight);
            }
            data_entry.last_updated = current_timestamp;
            data_entry.first_6_characters_of_sha3_256_hash_of_corresponding_file = report
                .first_6_characters_of_sha3_256_hash_of_corresponding_file
                .clone()
                .unwrap_or_default();
        }
        Err(index) => {
            // Insert new data at the correct position to keep the vector sorted
            let mut new_data = AggregatedConsensusData {
                txid: txid.to_string(),
                status_weights: [0; TXID_STATUS_VARIANT_COUNT],
                hash_weights: Vec::new(),
                first_6_characters_of_sha3_256_hash_of_corresponding_file: report
                    .first_6_characters_of_sha3_256_hash_of_corresponding_file
                    .clone()
                    .unwrap_or_default(),
                last_updated: current_timestamp,
            };
            new_data.status_weights[report.txid_status as usize] += scaled_weight;
            if let Some(hash) = &report.first_6_characters_of_sha3_256_hash_of_corresponding_file {
                new_data.hash_weights.push(HashWeight {
                    hash: hash.clone(),
                    weight: scaled_weight,
                });
            }
            consensus_data.insert(index, new_data);
        }
    }

    Ok(())
}

fn find_or_add_common_report_data(
    temp_report_account: &mut TempTxStatusReportAccount,
    common_data: &CommonReportData,
) -> u64 {
    // Use binary search directly without sorting
    match temp_report_account
        .common_reports
        .binary_search_by(|data| data.txid.cmp(&common_data.txid))
    {
        Ok(index) => index as u64,
        Err(index) => {
            temp_report_account
                .common_reports
                .insert(index, common_data.clone());
            index as u64
        }
    }
}

pub fn submit_data_report_helper(
    ctx: Context<SubmitDataReport>,
    txid: String,
    report: PastelTxStatusReport,
    contributor_reward_address: Pubkey,
) -> Result<()> {

    // Directly access accounts from the context
    let txid_submission_counts_account: &mut Account<'_, TxidSubmissionCountsAccount> =
        &mut ctx.accounts.txid_submission_counts_account;
    let aggregated_data_account = &mut ctx.accounts.aggregated_consensus_data_account;
    let temp_report_account = &mut ctx.accounts.temp_report_account;
    let contributor_data_account = &mut ctx.accounts.contributor_data_account;

    // Retrieve the submission count for the given txid from the PDA account
    let txid_submission_count: usize = txid_submission_counts_account
        .submission_counts
        .iter()
        .find(|c| c.txid == txid)
        .map_or(0, |c| c.count as usize);

    // Check if the number of submissions is already at or exceeds MIN_NUMBER_OF_ORACLES
    if txid_submission_count >= MIN_NUMBER_OF_ORACLES {
        msg!("Enough reports have already been submitted for this txid");
        return Err(OracleError::EnoughReportsSubmittedForTxid.into());
    }

    // Validate the data report before any contributor-specific checks
    // msg!("Validating data report: {:?}", report);
    validate_data_contributor_report(&report)?;

    // Check if the contributor is registered and not banned
    // msg!("Checking if contributor is registered and not banned");
    let contributor = contributor_data_account
        .contributors
        .iter()
        .find(|c| c.reward_address == contributor_reward_address)
        .ok_or(OracleError::ContributorNotRegistered)?;

    if contributor.calculate_is_banned(Clock::get()?.unix_timestamp as u32) {
        return Err(OracleError::ContributorBanned.into());
    }

    // Clone the String before using it
    let first_6_characters_of_sha3_256_hash_of_corresponding_file = report
        .first_6_characters_of_sha3_256_hash_of_corresponding_file
        .clone();

    // Extracting common data from the report
    // msg!("Extracting common data from the report");
    let common_data = CommonReportData {
        txid: report.txid.clone(),
        txid_status: report.txid_status,
        pastel_ticket_type: report.pastel_ticket_type,
        first_6_characters_of_sha3_256_hash_of_corresponding_file:
            first_6_characters_of_sha3_256_hash_of_corresponding_file,
    };

    // Finding or adding common report data
    // msg!("Finding or adding common report data");
    let common_data_index = find_or_add_common_report_data(temp_report_account, &common_data);

    // Creating specific report data
    // msg!("Creating specific report data");
    let specific_report = SpecificReportData {
        contributor_reward_address,
        timestamp: report.timestamp,
        common_data_ref: common_data_index,
    };

    // Creating a temporary report entry
    // msg!("Creating a temporary report entry");
    let temp_report: TempTxStatusReport = TempTxStatusReport {
        common_data_ref: common_data_index,
        specific_data: specific_report,
    };

    // Add the temporary report to the TempTxStatusReportAccount
    // msg!("Adding the temporary report to the TempTxStatusReportAccount");
    temp_report_account.reports.push(temp_report);

    // Update submission count and consensus-related data
    // msg!("Updating submission count and consensus-related data");
    update_submission_count(txid_submission_counts_account, &txid)?;

    let compliance_score = contributor.compliance_score;
    let reliability_score = contributor.reliability_score;
    let weight = compliance_score + reliability_score;
    aggregate_consensus_data(aggregated_data_account, &report, weight, &txid)?;

    // Check for consensus and perform related tasks
    if should_calculate_consensus(txid_submission_counts_account, &txid)? {
        msg!(
            "We now have enough reports to calculate consensus for txid: {}",
            txid
        );

        let contributor_data_account: &mut Account<'_, ContributorDataAccount> =
            &mut ctx.accounts.contributor_data_account;
        msg!("Calculating consensus...");
        calculate_consensus(
            aggregated_data_account,
            temp_report_account,
            contributor_data_account,
            &txid,
        )?;

        msg!("Performing post-consensus tasks...");
        post_consensus_tasks(
            txid_submission_counts_account,
            aggregated_data_account,
            temp_report_account,
            contributor_data_account,
            &txid,
        )?;
    }

    // Log the new size of temp_tx_status_reports
    msg!("New size of temp_tx_status_reports in bytes after processing report for txid {} from contributor {}: {}", txid, contributor_reward_address, temp_report_account.reports.len() * std::mem::size_of::<TempTxStatusReport>());

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
    txid: String,
    pending_payment: PendingPayment,
) -> Result<()> {
    let pending_payment_account = &mut ctx.accounts.pending_payment_account;

    // Ensure the account is being initialized for the first time to avoid re-initialization
    if !pending_payment_account.pending_payment.txid.is_empty()
        && pending_payment_account.pending_payment.txid != txid
    {
        return Err(OracleError::PendingPaymentAlreadyInitialized.into());
    }

    // Ensure txid is correct and other fields are properly set
    if pending_payment.txid != txid {
        return Err(OracleError::InvalidTxid.into());
    }

    // Store the pending payment in the account
    pending_payment_account.pending_payment = pending_payment;

    msg!(
        "Pending payment account initialized: TXID: {}, Expected Amount: {}, Status: {:?}",
        pending_payment_account.pending_payment.txid,
        pending_payment_account.pending_payment.expected_amount,
        pending_payment_account.pending_payment.payment_status
    );

    Ok(())
}

#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct Contributor {
    pub reward_address: Pubkey,
    pub registration_entrance_fee_transaction_signature: String,
    pub compliance_score: u64,
    pub last_active_timestamp: u32,
    pub total_reports_submitted: u32,
    pub accurate_reports_count: u32,
    pub current_streak: u32,
    pub reliability_score: u64,
    pub consensus_failures: u32,
    pub ban_expiry: u32,
    pub is_eligible_for_rewards: bool,
    pub is_recently_active: bool,
    pub is_reliable: bool,
}

#[account]
pub struct ContributorDataAccount {
    pub contributors: Vec<Contributor>,
}

#[account]
pub struct TxidSubmissionCountsAccount {
    pub submission_counts: Vec<TxidSubmissionCount>,
}

#[account]
pub struct AggregatedConsensusDataAccount {
    pub consensus_data: Vec<AggregatedConsensusData>,
}

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

// First set of accounts for main state initialization
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = user, space = 10_240)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

// Second set of accounts for PDA initialization
#[derive(Accounts)]
pub struct InitializePDAs<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        init,
        seeds = [b"temp_tx_status_report"],
        bump,
        payer = user,
        space = 10_240
    )]
    pub temp_report_account: Account<'info, TempTxStatusReportAccount>,

    #[account(
        init,
        seeds = [b"contributor_data"],
        bump,
        payer = user,
        space = 10_240
    )]
    pub contributor_data_account: Account<'info, ContributorDataAccount>,

    #[account(
        init,
        seeds = [b"txid_submission_counts"],
        bump,
        payer = user,
        space = 10_240
    )]
    pub txid_submission_counts_account: Account<'info, TxidSubmissionCountsAccount>,

    #[account(
        init,
        seeds = [b"aggregated_consensus_data"],
        bump,
        payer = user,
        space = 10_240
    )]
    pub aggregated_consensus_data_account: Account<'info, AggregatedConsensusDataAccount>,

    pub system_program: Program<'info, System>,
}

impl<'info> Initialize<'info> {
    pub fn initialize_oracle_state(&mut self) -> Result<()> {
        msg!("Setting up Oracle Contract State");

        let state = &mut self.oracle_contract_state;
        // Ensure the oracle_contract_state is not already initialized
        if state.is_initialized {
            return Err(OracleError::AccountAlreadyInitialized.into());
        }

        state.is_initialized = true;
        state.admin_pubkey = self.user.key();
        msg!("Admin Pubkey set to: {:?}", self.user.key());

        state.monitored_txids = Vec::new();
        msg!("Monitored Txids Vector initialized");

        state.bridge_contract_pubkey = Pubkey::default();
        msg!("Bridge Contract Pubkey set to default");

        msg!("Oracle Contract State Initialization Complete");
        Ok(())
    }
}

fn reallocate_temp_report_account<'info>(
    temp_report_account: &mut Account<'info, TempTxStatusReportAccount>,
    payer: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<()> {
    const REALLOCATION_THRESHOLD: f32 = 0.9;
    const ADDITIONAL_SPACE: usize = 10_240;
    const MAX_SIZE: usize = 100 * 1024; // 100KB

    let current_size = temp_report_account.to_account_info().data_len();
    let current_usage = temp_report_account.reports.len() * std::mem::size_of::<TempTxStatusReport>();
    let usage_ratio = current_usage as f32 / current_size as f32;

    if usage_ratio > REALLOCATION_THRESHOLD {
        let new_size = std::cmp::min(current_size + ADDITIONAL_SPACE, MAX_SIZE);

        // Prevent reallocating beyond MAX_SIZE
        if new_size > MAX_SIZE {
            msg!(
                "Cannot reallocate TempTxStatusReportAccount beyond MAX_SIZE of {} bytes.",
                MAX_SIZE
            );
            return Err(OracleError::MaxSizeExceeded.into());
        }

        // Attempt to reallocate
        temp_report_account.to_account_info().realloc(new_size, false)?;
        msg!(
            "TempTxStatusReportAccount reallocated from {} bytes to {} bytes.",
            current_size,
            new_size
        );

        // Calculate new rent minimum
        let rent = Rent::get()?;
        let new_rent_minimum = rent.minimum_balance(new_size);
        let current_lamports = temp_report_account.to_account_info().lamports();
        let lamports_needed = new_rent_minimum.saturating_sub(current_lamports);

        if lamports_needed > 0 {
            // Transfer lamports from payer to the account to meet rent exemption
            invoke(
                &system_instruction::transfer(
                    payer.key,
                    temp_report_account.to_account_info().key,
                    lamports_needed,
                ),
                &[
                    payer.clone(),
                    temp_report_account.to_account_info().clone(),
                    system_program.clone(),
                ],
            )?;
            msg!(
                "Transferred {} lamports from payer to TempTxStatusReportAccount to meet rent-exemption.",
                lamports_needed
            );
        }

        // Verify rent-exemption
        let updated_lamports = temp_report_account.to_account_info().lamports();
        let is_rent_exempt = rent.is_exempt(updated_lamports, new_size);
        if !is_rent_exempt {
            msg!(
                "TempTxStatusReportAccount is not rent-exempt after reallocation. Required: {}, Current: {}",
                new_rent_minimum,
                updated_lamports
            );
            return Err(OracleError::InsufficientFunds.into());
        }

        msg!(
            "TempTxStatusReportAccount is now rent-exempt with a size of {} bytes.",
            new_size
        );
    } else {
        msg!(
            "TempTxStatusReportAccount usage ratio ({:.2}) is below the reallocation threshold ({})",
            usage_ratio,
            REALLOCATION_THRESHOLD
        );
    }

    Ok(())
}

fn reallocate_contributor_data_account<'info>(
    contributor_data_account: &mut Account<'info, ContributorDataAccount>,
    payer: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<()> {
    const REALLOCATION_THRESHOLD: f32 = 0.9;
    const ADDITIONAL_SPACE: usize = 10_240;
    const MAX_SIZE: usize = 100 * 1024; // 100KB

    let current_size = contributor_data_account.to_account_info().data_len();
    let current_usage = contributor_data_account.contributors.len() * std::mem::size_of::<Contributor>();
    let usage_ratio = current_usage as f32 / current_size as f32;

    if usage_ratio > REALLOCATION_THRESHOLD {
        let new_size = std::cmp::min(current_size + ADDITIONAL_SPACE, MAX_SIZE);

        // Prevent reallocating beyond MAX_SIZE
        if new_size > MAX_SIZE {
            msg!(
                "Cannot reallocate ContributorDataAccount beyond MAX_SIZE of {} bytes.",
                MAX_SIZE
            );
            return Err(OracleError::MaxSizeExceeded.into());
        }

        // Attempt to reallocate
        contributor_data_account.to_account_info().realloc(new_size, false)?;
        msg!(
            "ContributorDataAccount reallocated from {} bytes to {} bytes.",
            current_size,
            new_size
        );

        // Calculate new rent minimum
        let rent = Rent::get()?;
        let new_rent_minimum = rent.minimum_balance(new_size);
        let current_lamports = contributor_data_account.to_account_info().lamports();
        let lamports_needed = new_rent_minimum.saturating_sub(current_lamports);

        if lamports_needed > 0 {
            // Transfer lamports from payer to the account to meet rent exemption
            invoke(
                &system_instruction::transfer(
                    payer.key,
                    contributor_data_account.to_account_info().key,
                    lamports_needed,
                ),
                &[
                    payer.clone(),
                    contributor_data_account.to_account_info().clone(),
                    system_program.clone(),
                ],
            )?;
            msg!(
                "Transferred {} lamports from payer to ContributorDataAccount to meet rent-exemption.",
                lamports_needed
            );
        }

        // Verify rent-exemption
        let updated_lamports = contributor_data_account.to_account_info().lamports();
        let is_rent_exempt = rent.is_exempt(updated_lamports, new_size);
        if !is_rent_exempt {
            msg!(
                "ContributorDataAccount is not rent-exempt after reallocation. Required: {}, Current: {}",
                new_rent_minimum,
                updated_lamports
            );
            return Err(OracleError::InsufficientFunds.into());
        }

        msg!(
            "ContributorDataAccount is now rent-exempt with a size of {} bytes.",
            new_size
        );
    } else {
        msg!(
            "ContributorDataAccount usage ratio ({:.2}) is below the reallocation threshold ({})",
            usage_ratio,
            REALLOCATION_THRESHOLD
        );
    }

    Ok(())
}

fn reallocate_submission_counts_account<'info>(
    submission_counts_account: &mut Account<'info, TxidSubmissionCountsAccount>,
    payer: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<()> {
    const REALLOCATION_THRESHOLD: f32 = 0.9;
    const ADDITIONAL_SPACE: usize = 10_240;
    const MAX_SIZE: usize = 100 * 1024; // 100KB

    let current_size = submission_counts_account.to_account_info().data_len();
    let current_usage = submission_counts_account
        .submission_counts
        .len()
        * std::mem::size_of::<TxidSubmissionCount>();
    let usage_ratio = current_usage as f32 / current_size as f32;

    if usage_ratio > REALLOCATION_THRESHOLD {
        let new_size = std::cmp::min(current_size + ADDITIONAL_SPACE, MAX_SIZE);

        // Prevent reallocating beyond MAX_SIZE
        if new_size > MAX_SIZE {
            msg!(
                "Cannot reallocate TxidSubmissionCountsAccount beyond MAX_SIZE of {} bytes.",
                MAX_SIZE
            );
            return Err(OracleError::MaxSizeExceeded.into());
        }

        // Attempt to reallocate
        submission_counts_account.to_account_info().realloc(new_size, false)?;
        msg!(
            "TxidSubmissionCountsAccount reallocated from {} bytes to {} bytes.",
            current_size,
            new_size
        );

        // Calculate new rent minimum
        let rent = Rent::get()?;
        let new_rent_minimum = rent.minimum_balance(new_size);
        let current_lamports = submission_counts_account.to_account_info().lamports();
        let lamports_needed = new_rent_minimum.saturating_sub(current_lamports);

        if lamports_needed > 0 {
            // Transfer lamports from payer to the account to meet rent exemption
            invoke(
                &system_instruction::transfer(
                    payer.key,
                    submission_counts_account.to_account_info().key,
                    lamports_needed,
                ),
                &[
                    payer.clone(),
                    submission_counts_account.to_account_info().clone(),
                    system_program.clone(),
                ],
            )?;
            msg!(
                "Transferred {} lamports from payer to TxidSubmissionCountsAccount to meet rent-exemption.",
                lamports_needed
            );
        }

        // Verify rent-exemption
        let updated_lamports = submission_counts_account.to_account_info().lamports();
        let is_rent_exempt = rent.is_exempt(updated_lamports, new_size);
        if !is_rent_exempt {
            msg!(
                "TxidSubmissionCountsAccount is not rent-exempt after reallocation. Required: {}, Current: {}",
                new_rent_minimum,
                updated_lamports
            );
            return Err(OracleError::InsufficientFunds.into());
        }

        msg!(
            "TxidSubmissionCountsAccount is now rent-exempt with a size of {} bytes.",
            new_size
        );
    } else {
        msg!(
            "TxidSubmissionCountsAccount usage ratio ({:.2}) is below the reallocation threshold ({})",
            usage_ratio,
            REALLOCATION_THRESHOLD
        );
    }

    Ok(())
}

fn reallocate_aggregated_consensus_data_account<'info>(
    aggregated_consensus_data_account: &mut Account<'info, AggregatedConsensusDataAccount>,
    payer: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<()> {
    const REALLOCATION_THRESHOLD: f32 = 0.9;
    const ADDITIONAL_SPACE: usize = 10_240;
    const MAX_SIZE: usize = 100 * 1024; // 100KB

    let current_size = aggregated_consensus_data_account.to_account_info().data_len();
    let current_usage = aggregated_consensus_data_account
        .consensus_data
        .len()
        * std::mem::size_of::<AggregatedConsensusData>();
    let usage_ratio = current_usage as f32 / current_size as f32;

    if usage_ratio > REALLOCATION_THRESHOLD {
        let new_size = std::cmp::min(current_size + ADDITIONAL_SPACE, MAX_SIZE);

        // Prevent reallocating beyond MAX_SIZE
        if new_size > MAX_SIZE {
            msg!(
                "Cannot reallocate AggregatedConsensusDataAccount beyond MAX_SIZE of {} bytes.",
                MAX_SIZE
            );
            return Err(OracleError::MaxSizeExceeded.into());
        }

        // Attempt to reallocate
        aggregated_consensus_data_account.to_account_info().realloc(new_size, false)?;
        msg!(
            "AggregatedConsensusDataAccount reallocated from {} bytes to {} bytes.",
            current_size,
            new_size
        );

        // Calculate new rent minimum
        let rent = Rent::get()?;
        let new_rent_minimum = rent.minimum_balance(new_size);
        let current_lamports = aggregated_consensus_data_account.to_account_info().lamports();
        let lamports_needed = new_rent_minimum.saturating_sub(current_lamports);

        if lamports_needed > 0 {
            // Transfer lamports from payer to the account to meet rent exemption
            invoke(
                &system_instruction::transfer(
                    payer.key,
                    aggregated_consensus_data_account.to_account_info().key,
                    lamports_needed,
                ),
                &[
                    payer.clone(),
                    aggregated_consensus_data_account.to_account_info().clone(),
                    system_program.clone(),
                ],
            )?;
            msg!(
                "Transferred {} lamports from payer to AggregatedConsensusDataAccount to meet rent-exemption.",
                lamports_needed
            );
        }

        // Verify rent-exemption
        let updated_lamports = aggregated_consensus_data_account.to_account_info().lamports();
        let is_rent_exempt = rent.is_exempt(updated_lamports, new_size);
        if !is_rent_exempt {
            msg!(
                "AggregatedConsensusDataAccount is not rent-exempt after reallocation. Required: {}, Current: {}",
                new_rent_minimum,
                updated_lamports
            );
            return Err(OracleError::InsufficientFunds.into());
        }

        msg!(
            "AggregatedConsensusDataAccount is now rent-exempt with a size of {} bytes.",
            new_size
        );
    } else {
        msg!(
            "AggregatedConsensusDataAccount usage ratio ({:.2}) is below the reallocation threshold ({})",
            usage_ratio,
            REALLOCATION_THRESHOLD
        );
    }

    Ok(())
}

#[derive(Accounts)]
pub struct ReallocateOracleState<'info> {
    #[account(mut, has_one = admin_pubkey)]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    pub admin_pubkey: Signer<'info>, // Admin is the payer

    pub system_program: Program<'info, System>,

    #[account(mut)]
    pub temp_report_account: Account<'info, TempTxStatusReportAccount>,

    #[account(mut)]
    pub contributor_data_account: Account<'info, ContributorDataAccount>,

    #[account(mut)]
    pub txid_submission_counts_account: Account<'info, TxidSubmissionCountsAccount>,

    #[account(mut)]
    pub aggregated_consensus_data_account: Account<'info, AggregatedConsensusDataAccount>,
}

impl<'info> ReallocateOracleState<'info> {
    pub fn execute(ctx: Context<ReallocateOracleState>) -> Result<()> {
        let payer = ctx.accounts.admin_pubkey.to_account_info(); // Admin is the payer
        let system_program = ctx.accounts.system_program.to_account_info();

        // Reallocate TempTxStatusReportAccount
        reallocate_temp_report_account(
            &mut ctx.accounts.temp_report_account,
            &payer,
            &system_program,
        )?;

        // Reallocate ContributorDataAccount
        reallocate_contributor_data_account(
            &mut ctx.accounts.contributor_data_account,
            &payer,
            &system_program,
        )?;

        // Reallocate TxidSubmissionCountsAccount
        reallocate_submission_counts_account(
            &mut ctx.accounts.txid_submission_counts_account,
            &payer,
            &system_program,
        )?;

        // Reallocate AggregatedConsensusDataAccount
        reallocate_aggregated_consensus_data_account(
            &mut ctx.accounts.aggregated_consensus_data_account,
            &payer,
            &system_program,
        )?;

        msg!("All accounts reallocated and rent-exempt status ensured.");
        Ok(())
    }
}

#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct HashWeight {
    pub hash: String,
    pub weight: u64,
}

fn update_hash_weight(hash_weights: &mut Vec<HashWeight>, hash: &str, weight: u64) {
    if let Some(hash_weight) = hash_weights.iter_mut().find(|hw| hw.hash == hash) {
        hash_weight.weight += weight;
    } else {
        hash_weights.push(HashWeight {
            hash: hash.to_string(),
            weight,
        });
    }
}

// Struct to hold aggregated data for consensus calculation
#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct AggregatedConsensusData {
    pub txid: String,
    pub status_weights: [u64; TXID_STATUS_VARIANT_COUNT],
    pub hash_weights: Vec<HashWeight>,
    pub first_6_characters_of_sha3_256_hash_of_corresponding_file: String,
    pub last_updated: u32, // Unix timestamp indicating the last update time
}

#[derive(Accounts)]
pub struct RequestReward<'info> {
    /// CHECK: OK
    #[account(mut, seeds = [b"reward_pool"], bump)]
    pub reward_pool_account: UncheckedAccount<'info>,
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    #[account(mut)]
    pub contributor_data_account: Account<'info, ContributorDataAccount>,
    /// CHECK: This is the account we're transferring lamports to
    #[account(mut)]
    pub contributor: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn request_reward_helper(
    ctx: Context<RequestReward>,
    contributor_address: Pubkey,
) -> Result<()> {
    let contributor_data_account = &ctx.accounts.contributor_data_account;
    let reward_pool_account = &ctx.accounts.reward_pool_account;
    let contributor_account = &ctx.accounts.contributor;

    // Find the contributor in the PDA and check eligibility
    let contributor = contributor_data_account
        .contributors
        .iter()
        .find(|c| c.reward_address == contributor_address)
        .ok_or(OracleError::UnregisteredOracle)?;

    let current_unix_timestamp = Clock::get()?.unix_timestamp as u32;

    if !contributor.is_eligible_for_rewards {
        msg!(
            "Contributor is not eligible for rewards: {}",
            contributor_address
        );
        return Err(OracleError::NotEligibleForReward.into());
    }

    if contributor.calculate_is_banned(current_unix_timestamp) {
        msg!("Contributor is banned: {}", contributor_address);
        return Err(OracleError::ContributorBanned.into());
    }

    let reward_amount = BASE_REWARD_AMOUNT_IN_LAMPORTS;

    // Ensure the reward pool has sufficient funds
    if reward_pool_account.to_account_info().lamports() < reward_amount {
        msg!("Insufficient funds in reward pool");
        return Err(OracleError::InsufficientFunds.into());
    }

    // Transfer the reward from the reward pool to the contributor
    transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.reward_pool_account.to_account_info(),
                to: contributor_account.to_account_info(),
            },
        )
        .with_signer(&[&[b"reward_pool", &[ctx.bumps.reward_pool_account]]]),
        reward_amount,
    )?;

    msg!(
        "Paid out Valid Reward Request: Contributor: {}, Amount: {}",
        contributor_address,
        reward_amount
    );

    Ok(())
}

#[derive(Accounts)]
pub struct RegisterNewDataContributor<'info> {
    /// CHECK: Manual checks are performed in the instruction to ensure the contributor_account is valid and safe to use.
    #[account(mut)]
    pub contributor_account: Signer<'info>,

    /// CHECK: OK
    #[account(mut, seeds = [b"reward_pool"], bump)]
    pub reward_pool_account: UncheckedAccount<'info>,

    /// CHECK: OK
    #[account(mut, seeds = [b"fee_receiving_contract"], bump)]
    pub fee_receiving_contract_account: UncheckedAccount<'info>,

    #[account(mut)]
    pub contributor_data_account: Account<'info, ContributorDataAccount>,

    pub system_program: Program<'info, System>,
}

pub fn register_new_data_contributor_helper(
    ctx: Context<RegisterNewDataContributor>,
) -> Result<()> {
    let contributor_data_account = &mut ctx.accounts.contributor_data_account;
    let reward_address = ctx.accounts.contributor_account.key();

    // Ensure the contributors list is sorted by reward_address
    contributor_data_account
        .contributors
        .sort_by(|a, b| a.reward_address.cmp(&b.reward_address));

    // Check if the contributor is already registered using binary search
    if contributor_data_account
        .contributors
        .binary_search_by(|c| c.reward_address.cmp(&reward_address))
        .is_ok()
    {
        return Err(OracleError::ContributorAlreadyRegistered.into());
    }

    // Deduct the registration fee from the fee_receiving_contract_account and add it to the reward pool account
    transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx
                    .accounts
                    .fee_receiving_contract_account
                    .to_account_info(),
                to: ctx.accounts.reward_pool_account.to_account_info(),
            },
        )
        .with_signer(&[&[
            b"fee_receiving_contract",
            &[ctx.bumps.fee_receiving_contract_account],
        ]]),
        REGISTRATION_ENTRANCE_FEE_IN_LAMPORTS as u64,
    )?;

    let last_active_timestamp = Clock::get()?.unix_timestamp as u32;

    let new_contributor = Contributor {
        reward_address,
        registration_entrance_fee_transaction_signature: String::new(),
        compliance_score: ONE,
        last_active_timestamp,
        total_reports_submitted: 0,
        accurate_reports_count: 0,
        current_streak: 0,
        reliability_score: ONE,
        consensus_failures: 0,
        ban_expiry: 0,
        is_eligible_for_rewards: false,
        is_recently_active: false,
        is_reliable: false,
    };

    // Insert the new contributor at the correct position to keep the list sorted
    match contributor_data_account
        .contributors
        .binary_search_by(|c| c.reward_address.cmp(&new_contributor.reward_address))
    {
        Ok(_) => {} // Should not happen since we checked earlier
        Err(index) => contributor_data_account.contributors.insert(index, new_contributor),
    }

    Ok(())
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct AddTxidForMonitoringData {
    pub txid: String,
}

#[derive(Accounts)]
pub struct AddTxidForMonitoring<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    /// CHECK: The caller is manually verified in the instruction logic to ensure it's the correct and authorized account.
    pub caller: Signer<'info>,

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

#[derive(Accounts)]
pub struct ProcessPastelTxStatusReport<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    /// CHECK: Manual checks are performed in the instruction to ensure the contributor is valid and authorized. This includes verifying signatures and other relevant validations.
    #[account(mut)]
    pub contributor: Signer<'info>,
    // You can add other accounts as needed
}

pub fn should_calculate_consensus(
    txid_submission_counts_account: &Account<TxidSubmissionCountsAccount>,
    txid: &str,
) -> Result<bool> {
    let current_unix_timestamp = Clock::get()?.unix_timestamp as u32;

    // Ensure the vector is sorted by txid
    let counts = &txid_submission_counts_account.submission_counts;
    match counts.binary_search_by(|c| c.txid.as_str().cmp(txid)) {
        Ok(index) => {
            let count = &counts[index];
            let submission_count = count.count;
            let last_updated = count.last_updated;

            let min_threshold_met = submission_count >= MIN_NUMBER_OF_ORACLES as u32;
            let time_elapsed = current_unix_timestamp.saturating_sub(last_updated);
            let max_waiting_period_elapsed = time_elapsed >= MAX_DURATION_IN_SECONDS_FROM_LAST_REPORT_SUBMISSION_BEFORE_COMPUTING_CONSENSUS;

            Ok(min_threshold_met || (max_waiting_period_elapsed && submission_count >= MIN_NUMBER_OF_ORACLES as u32))
        }
        Err(_) => Ok(false),
    }
}

pub fn cleanup_old_submission_counts(
    txid_submission_counts_account: &mut Account<TxidSubmissionCountsAccount>,
    current_timestamp: u32,
) {
    let retention_period = SUBMISSION_COUNT_RETENTION_PERIOD;
    txid_submission_counts_account
        .submission_counts
        .retain(|count| current_timestamp.saturating_sub(count.last_updated) < retention_period);
}

pub fn usize_to_txid_status(index: usize) -> Option<TxidStatus> {
    match index {
        0 => Some(TxidStatus::Invalid),
        1 => Some(TxidStatus::PendingMining),
        2 => Some(TxidStatus::MinedPendingActivation),
        3 => Some(TxidStatus::MinedActivated),
        _ => None,
    }
}

// Function to handle the submission of Pastel transaction status reports
pub fn validate_data_contributor_report(report: &PastelTxStatusReport) -> Result<()> {
    // Direct return in case of invalid data, reducing nested if conditions
    if report.txid.trim().is_empty() {
        msg!("Error: InvalidTxid (TXID is empty)");
        return Err(OracleError::InvalidTxid.into());
    }
    // Simplified TXID status validation
    if !matches!(
        report.txid_status,
        TxidStatus::MinedActivated
            | TxidStatus::MinedPendingActivation
            | TxidStatus::PendingMining
            | TxidStatus::Invalid
    ) {
        return Err(OracleError::InvalidTxidStatus.into());
    }
    // Direct return in case of missing data, reducing nested if conditions
    if report.pastel_ticket_type.is_none() {
        msg!("Error: Missing Pastel Ticket Type");
        return Err(OracleError::MissingPastelTicketType.into());
    }
    // Direct return in case of invalid hash, reducing nested if conditions
    if let Some(hash) = &report.first_6_characters_of_sha3_256_hash_of_corresponding_file {
        if hash.len() != 6 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            msg!("Error: Invalid File Hash Length or Non-hex characters");
            return Err(OracleError::InvalidFileHashLength.into());
        }
    } else {
        return Err(OracleError::MissingFileHash.into());
    }
    Ok(())
}

impl Contributor {
    // Check if the contributor is currently banned
    pub fn calculate_is_banned(&self, current_time: u32) -> bool {
        current_time < self.ban_expiry
    }

    // Method to determine if the contributor is eligible for rewards
    pub fn calculate_is_eligible_for_rewards(&self) -> bool {
        self.total_reports_submitted >= MIN_REPORTS_FOR_REWARD
            && self.reliability_score >= MIN_RELIABILITY_SCORE_FOR_REWARD
            && self.compliance_score >= MIN_COMPLIANCE_SCORE_FOR_REWARD
    }
}

#[derive(Accounts)]
pub struct SetBridgeContract<'info> {
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    pub admin_pubkey: Signer<'info>,
}

impl<'info> SetBridgeContract<'info> {
    pub fn set_bridge_contract(
        ctx: Context<SetBridgeContract>,
        bridge_contract_pubkey: Pubkey,
    ) -> Result<()> {
        let state = &mut ctx.accounts.oracle_contract_state;
        // Explicit admin check
        if state.admin_pubkey != ctx.accounts.admin_pubkey.key() {
            return Err(OracleError::UnauthorizedWithdrawalAccount.into());
        }
        state.bridge_contract_pubkey = bridge_contract_pubkey;
        msg!(
            "Bridge contract pubkey updated: {:?}",
            bridge_contract_pubkey
        );
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(txid: String)] // Include txid as part of the instruction
pub struct ProcessPayment<'info> {
    /// CHECK: This is checked in the handler function to verify it's the bridge contract.
    pub source_account: Signer<'info>,

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
    txid: String,
    amount: u64,
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
    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,
    
    /// CHECK: The admin_account is manually verified in the instruction
    #[account(mut)]
    pub admin_account: Signer<'info>,

    /// CHECK: OK
    #[account(mut, seeds = [b"reward_pool"], bump)]
    pub reward_pool_account: UncheckedAccount<'info>,
    /// CHECK: OK
    #[account(mut, seeds = [b"fee_receiving_contract"], bump)]
    pub fee_receiving_contract_account: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

impl<'info> WithdrawFunds<'info> {
    pub fn execute(
        ctx: Context<WithdrawFunds>,
        reward_pool_amount: u64,
        fee_receiving_amount: u64,
    ) -> Result<()> {
        // Explicit admin check
        if ctx.accounts.oracle_contract_state.admin_pubkey != ctx.accounts.admin_account.key() {
            return Err(OracleError::UnauthorizedWithdrawalAccount.into());
        }

        let reward_pool_account = &mut ctx.accounts.reward_pool_account;
        let fee_receiving_contract_account = &mut ctx.accounts.fee_receiving_contract_account;

        // Transfer SOL from the reward pool account to the admin account
        if reward_pool_account.lamports() < reward_pool_amount {
            return Err(OracleError::InsufficientFunds.into());
        }
        transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.reward_pool_account.to_account_info(),
                    to: ctx.accounts.admin_account.to_account_info(),
                },
            )
            .with_signer(&[&[b"reward_pool", &[ctx.bumps.reward_pool_account]]]),
            reward_pool_amount,
        )?;

        // Transfer SOL from the fee receiving contract account to the admin account
        if fee_receiving_contract_account.lamports() < fee_receiving_amount {
            return Err(OracleError::InsufficientFunds.into());
        }
        transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: fee_receiving_contract_account.to_account_info(),
                    to: ctx.accounts.admin_account.to_account_info(),
                },
            )
            .with_signer(&[&[
                b"fee_receiving_contract",
                &[ctx.bumps.fee_receiving_contract_account],
            ]]),
            fee_receiving_amount,
        )?;

        msg!("Withdrawal successful: {} lamports transferred from reward pool and {} lamports from fee receiving contract to admin account", reward_pool_amount, fee_receiving_amount);
        Ok(())
    }
}

declare_id!("AfP1c4sFcY1FeiGjQEtyxCim8BRnw22okNbKAsH2sBsB");

#[program]
pub mod solana_pastel_oracle_program {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Initializing Oracle Contract State");
        let state = &mut ctx.accounts.oracle_contract_state;
        
        if state.is_initialized {
            return Err(OracleError::AccountAlreadyInitialized.into());
        }

        state.is_initialized = true;
        state.admin_pubkey = ctx.accounts.user.key();
        state.monitored_txids = Vec::new();
        state.bridge_contract_pubkey = Pubkey::default();

        msg!("Oracle Contract State Initialization Complete");
        Ok(())
    }

    pub fn initialize_pdas(ctx: Context<InitializePDAs>) -> Result<()> {
        // Initialize TempTxStatusReportAccount
        ctx.accounts.temp_report_account.reports = Vec::new();
        ctx.accounts.temp_report_account.common_reports = Vec::new();
        ctx.accounts.temp_report_account.specific_reports = Vec::new();

        // Initialize ContributorDataAccount
        ctx.accounts.contributor_data_account.contributors = Vec::new();

        // Initialize TxidSubmissionCountsAccount
        ctx.accounts.txid_submission_counts_account.submission_counts = Vec::new();

        // Initialize AggregatedConsensusDataAccount
        ctx.accounts.aggregated_consensus_data_account.consensus_data = Vec::new();

        msg!("PDA Accounts Initialization Complete");
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

        let timestamp = Clock::get()?.unix_timestamp as u32;

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
        let oracle_state = &ctx.accounts.oracle_contract_state;
        let admin = &ctx.accounts.admin_account;
        if oracle_state.admin_pubkey != admin.key() {
            return Err(OracleError::UnauthorizedWithdrawalAccount.into());
        }
        WithdrawFunds::execute(ctx, reward_pool_amount, fee_receiving_amount)
    }
}
