use anchor_lang::prelude::*;
use anchor_lang::solana_program::entrypoint::ProgramResult;
use anchor_lang::solana_program::account_info::AccountInfo;
use anchor_lang::solana_program::sysvar::clock::Clock;
use std::collections::BTreeMap;
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
const MAX_DURATION_IN_SECONDS_FROM_LAST_REPORT_SUBMISSION_BEFORE_COMPUTING_CONSENSUS: u64 = 20 * 60; // Maximum duration in seconds from last report submission for a given TXID before computing consensus (e.g., 20 minutes)
const CONTRIBUTOR_RETENTION_PERIOD: u64 = 30 * 24 * 60 * 60; // How long to keep contributor data in the contract state when they've been inactive (30 days)
const SUBMISSION_COUNT_RETENTION_PERIOD: u64 = 86_400; // Number of seconds to retain submission counts (i.e., 24 hours)
const TXID_STATUS_VARIANT_COUNT: usize = 4; // Manually define the number of variants in TxidStatus

#[error_code]
pub enum OracleError {
    ContributorAlreadyRegistered,
    UnregisteredOracle,
    InvalidTxid,
    InvalidFileHashLength,
    SubmissionCountExceeded,
    MissingPastelTicketType,
    MissingFileHash,
    InvalidTimestamp,
    RegistrationFeeNotPaid,
    NotEligibleForReward,
    InvalidAccountData,
    InsufficientFunds,
    Unauthorized,
    InvalidPaymentAmount,
    PaymentNotFound,
    InvalidOperation
}

impl From<OracleError> for ProgramError {
    fn from(e: OracleError) -> Self {
        ProgramError::Custom(e as u32)
    }
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
pub struct PendingPayment {
    pub txid: String,
    pub expected_amount: u64,
    pub payment_status: PaymentStatus,
}

#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
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
        seeds = [b"pastel_tx_status_report", txid.as_bytes(), reward_address.as_ref()],
        bump,
        space = 8 + (32 + 1 + 4 + 200 + 8 + 32) // Adjust the space as needed
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
    pub txid: String,
    pub status: String,
    pub timestamp: u64,
}

// Function to update the submission count for a given txid
fn update_submission_count(state: &mut OracleContractState, txid: &str) -> Result<()> {
    let current_timestamp = Clock::get()?.unix_timestamp;
    let current_timestamp_u64 = current_timestamp.try_into().map_err(|_| OracleError::InvalidTimestamp)?;

    let mut found = false;

    for count in &mut state.txid_submission_counts {
        if count.txid == txid {
            count.count += 1;
            count.last_updated = current_timestamp_u64;
            found = true;
            break;
        }
    }

    if !found {
        state.txid_submission_counts.push(TxidSubmissionCount {
            txid: txid.to_string(),
            count: 1,
            last_updated: current_timestamp_u64,
        });
    }

    Ok(())
}

pub fn submit_data_report_helper(ctx: Context<SubmitDataReport>, txid: String, report: PastelTxStatusReport) -> ProgramResult {
    let contributor_key = ctx.accounts.user.key();
    let state = &mut ctx.accounts.oracle_contract_state;
    let report_account = &mut ctx.accounts.report_account;

    // Check if the contributor's key matches the report's contributor_reward_address
    if report.contributor_reward_address != contributor_key {
        return Err(OracleError::Unauthorized.into());
    }

    // Validate the report
    validate_data_contributor_report(&report)?;

    // Check if the contributor is registered, not banned, and store compliance score
    let mut contributor_compliance_score = 0;
    let current_timestamp = Clock::get()?.unix_timestamp.try_into().map_err(|_| OracleError::InvalidTimestamp)?;
    let contributor_registered = state.contributors.iter().any(|c| {
        if c.reward_address == contributor_key {
            if c.calculate_is_banned(current_timestamp) {
                return false;
            }
            contributor_compliance_score = c.compliance_score;
            true
        } else {
            false
        }
    });

    if !contributor_registered {
        return Err(OracleError::UnregisteredOracle.into());
    }

    // Protect against re-initialization attack
    if report_account.report.txid != "" && report_account.report.txid != txid {
        return Err(OracleError::InvalidOperation.into());
    }

    // Store the report in the PastelTxStatusReportAccount PDA
    // Clone report for later use
    let report_clone = report.clone();
    report_account.report = report;

    // Update the submission count for the given txid
    update_submission_count(state, &txid)?;

    // Calculate weight
    let weight = normalize_compliance_score(contributor_compliance_score);

    // Emit an event after storing the report
    emit!(DataReportSubmitted {
        contributor: contributor_key,
        txid: txid.clone(),
        timestamp: current_timestamp,
        status: "Report Submitted".to_string(),
    });

    // Aggregate data for consensus calculation
    let mut found = false;
    for data in &mut state.aggregated_consensus_data {
        if data.txid == txid {
            data.status_weights[report_clone.txid_status as usize] += weight;
            if let Some(hash) = &report_clone.first_6_characters_of_sha3_256_hash_of_corresponding_file {
                *data.hash_weights.entry(hash.clone()).or_insert(0) += weight;
            }
            found = true;
            break;
        }
    }

    if !found {
        let mut new_data = AggregatedConsensusData {
            txid: txid.clone(),
            status_weights: [0; TXID_STATUS_VARIANT_COUNT],
            hash_weights: BTreeMap::new(),
        };
        new_data.status_weights[report_clone.txid_status as usize] += weight;
        if let Some(hash) = &report_clone.first_6_characters_of_sha3_256_hash_of_corresponding_file {
            new_data.hash_weights.insert(hash.clone(), weight);
        }
        state.aggregated_consensus_data.push(new_data);
    }

    // Calculate consensus and cleanup if necessary
    if should_calculate_consensus(state, &txid)? {
        calculate_consensus_and_cleanup(ctx.program_id, state, &txid)?;
    }

    // Cleanup old submission counts
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
        seeds = [b"pending_payment", txid.as_bytes()],
        bump,
        space = 8 + std::mem::size_of::<PendingPayment>() // Adjust the space as needed
    )]
    pub pending_payment_account: Account<'info, PendingPaymentAccount>,

    #[account(mut)]
    pub oracle_contract_state: Account<'info, OracleContractState>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn add_pending_payment_helper(ctx: Context<HandlePendingPayment>, txid: String, pending_payment: PendingPayment) -> ProgramResult {
    let pending_payment_account = &mut ctx.accounts.pending_payment_account;

    // Ensure the account is being initialized for the first time to avoid re-initialization
    if pending_payment_account.pending_payment.txid != "" && pending_payment_account.pending_payment.txid != txid {
        msg!("Attempted to re-initialize an already initialized pending payment account.");
        return Err(OracleError::InvalidOperation.into());
    }

    // Store the pending payment in the account
    pending_payment_account.pending_payment = pending_payment;

    Ok(())
}

#[derive(Accounts)]
#[instruction(admin_pubkey: Pubkey)]
pub struct Initialize<'info> {
    #[account(init, payer = user, space = 8 + 50000)] // Adjust the space as needed
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
    pub aggregated_consensus_data: Vec<AggregatedConsensusData>,
    pub reward_pool_account: Pubkey,
    pub reward_pool_nonce: u8,
    pub fee_receiving_contract_account: Pubkey,
    pub fee_receiving_contract_nonce: u8,
    pub bridge_contract_pubkey: Pubkey,
}

// Struct to hold aggregated data for consensus calculation
#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct AggregatedConsensusData {
    pub txid: String,
    pub status_weights: [i32; TXID_STATUS_VARIANT_COUNT],
    pub hash_weights: BTreeMap<String, i32>,
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

#[event]
pub struct RewardEvent {
    pub contributor: Pubkey,
    pub amount: u64,
    pub status: String,
}

pub fn request_reward_helper(ctx: Context<RequestReward>) -> Result<()> {
    let contributor = &mut ctx.accounts.contributor;

    msg!("Attempting to request reward for contributor: {}", contributor.reward_address);
    // Convert i64 Unix timestamp to u64
    let current_unix_timestamp = Clock::get()?.unix_timestamp as u64;

    // Store eligibility and ban status in temporary variables
    let is_eligible_for_rewards = contributor.is_eligible_for_rewards;
    let is_banned = contributor.calculate_is_banned(current_unix_timestamp);

    // Check if the contributor is eligible for a reward
    if is_eligible_for_rewards && !is_banned {
        // Calculate the reward amount for the contributor
        let reward_amount = BASE_REWARD_AMOUNT_SOL; // Adjust based on your logic

        // Transfer the reward from the reward pool to the contributor
        **contributor.to_account_info().lamports.borrow_mut() += reward_amount;
        **ctx.accounts.reward_pool_account.to_account_info().lamports.borrow_mut() -= reward_amount;

        // Emit event for valid reward request
        emit!(RewardEvent {
            contributor: contributor.reward_address,
            amount: reward_amount,
            status: "Valid Reward Paid".to_string(),
        });

        // Update state to reflect the reward distribution
        msg!("Paid out Valid Reward Request: Contributor: {}, Amount: {}", contributor.reward_address, reward_amount);

    } else {
        // Emit event for invalid reward request
        emit!(RewardEvent {
            contributor: contributor.reward_address,
            amount: 0,
            status: format!("Invalid Reward Request: Eligible: {}, Banned: {}", is_eligible_for_rewards, is_banned),
        });

        msg!("Invalid Reward Request: Contributor: {}, Eligible: {}, Banned: {}", contributor.reward_address, is_eligible_for_rewards, is_banned);
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

    msg!("Checking registration fee payment for new contributor: {}", ctx.accounts.contributor_account.key);
    // Check if the fee_receiving_contract_account received the registration fee
    if ctx.accounts.fee_receiving_contract_account.to_account_info().lamports() < REGISTRATION_ENTRANCE_FEE_SOL {
        return Err(OracleError::RegistrationFeeNotPaid.into());
    }

    msg!("Registration fee verified. Attempting to registering new contributor {}", ctx.accounts.contributor_account.key);

    // Transfer the fee to the reward pool account
    **ctx.accounts.reward_pool_account.to_account_info().lamports.borrow_mut() += ctx.accounts.fee_receiving_contract_account.to_account_info().lamports();
    **ctx.accounts.fee_receiving_contract_account.to_account_info().lamports.borrow_mut() = 0;

    let last_active_timestamp = Clock::get()?.unix_timestamp as u64;
    // Create and add the new contributor
    let new_contributor = Contributor {
        reward_address: *ctx.accounts.contributor_account.key,
        registration_entrance_fee_transaction_signature: String::new(), // Replace with actual data if available
        compliance_score: 0, // Initial compliance score
        last_active_timestamp: last_active_timestamp, // Set the last active timestamp to the current time
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
    contributor.is_recently_active = contributor.calculate_is_recently_active(current_timestamp);
    contributor.is_reliable = contributor.calculate_is_reliable();
    contributor.is_eligible_for_rewards = contributor.calculate_is_eligible_for_rewards();

    // Emit event for contributor update
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

    msg!("Contributor {} updated: Last Active Timestamp: {}, Total Reports Submitted: {}, Accurate Reports Count: {}, Current Streak: {}, Compliance Score: {}, Reliability Score: {}, Consensus Failures: {}, Ban Expiry: {}, Is Recently Active: {}, Is Reliable: {}, Is Eligible For Rewards: {}", contributor.reward_address, contributor.last_active_timestamp, contributor.total_reports_submitted, contributor.accurate_reports_count, contributor.current_streak, contributor.compliance_score, contributor.reliability_score, contributor.consensus_failures, contributor.ban_expiry, contributor.is_recently_active, contributor.is_reliable, contributor.is_eligible_for_rewards);

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
    pub txid: String,
    pub expected_amount: u64,
}

pub fn add_txid_for_monitoring_helper(ctx: Context<AddTxidForMonitoring>, data: AddTxidForMonitoringData) -> Result<()> {
    let state = &mut ctx.accounts.oracle_contract_state;

    if ctx.accounts.caller.key != &state.bridge_contract_pubkey {
        return Err(OracleError::InvalidAccountData.into());
    }

    let txid = data.txid.clone();

    // Add the TXID to the monitored list
    state.monitored_txids.push(txid.clone());

    // Initialize pending_payment_account here using the txid
    let pending_payment_account = &mut ctx.accounts.pending_payment_account;
    pending_payment_account.pending_payment = PendingPayment {
        txid: txid.clone(),
        expected_amount: COST_IN_SOL_OF_ADDING_PASTEL_TXID_FOR_MONITORING,
        payment_status: PaymentStatus::Pending,
    };

    // Emit an event for adding TXID for monitoring
    emit!(TxidAddedForMonitoringEvent {
        txid: txid.clone(),
        expected_amount: COST_IN_SOL_OF_ADDING_PASTEL_TXID_FOR_MONITORING,
    });

    msg!("Added Pastel TXID for Monitoring and Pending Payment: {}", txid);
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
        state.bridge_contract_pubkey = Pubkey::default(); // Initialize with default or actual value
        Ok(())
    }
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

fn should_calculate_consensus(state: &OracleContractState, txid: &str) -> Result<bool> {
    // Retrieve the count of submissions and last updated timestamp for the given txid
    let (submission_count, last_updated) = state.txid_submission_counts.iter()
        .find(|c| c.txid == txid)
        .map(|c| (c.count, c.last_updated))
        .unwrap_or((0, 0));

    // Calculate the aspirational minimum target based on reliable and active contributors
    let active_reliable_contributors = state.contributors.iter()
        .filter(|c| c.calculate_is_recently_active(last_updated) && c.calculate_is_reliable())
        .count();

    // Define the minimum number of reports required before attempting to calculate consensus
    let min_reports_for_consensus = std::cmp::max(MIN_NUMBER_OF_ORACLES, active_reliable_contributors);

    // Check if the minimum threshold of reports is met
    let min_threshold_met = submission_count >= min_reports_for_consensus as u32;

    // Get the current unix timestamp from the Solana clock
    let current_unix_timestamp = Clock::get()?.unix_timestamp as u64;

    // Check if 20 minutes have elapsed since the last update
    let max_waiting_period_elapsed_for_txid = current_unix_timestamp - last_updated >= MAX_DURATION_IN_SECONDS_FROM_LAST_REPORT_SUBMISSION_BEFORE_COMPUTING_CONSENSUS;

    // Calculate consensus if minimum threshold is met or if 20 minutes have passed with at least MIN_NUMBER_OF_ORACLES reports
    Ok(min_threshold_met || (max_waiting_period_elapsed_for_txid && submission_count >= MIN_NUMBER_OF_ORACLES as u32))
}

fn cleanup_old_submission_counts(state: &mut OracleContractState) -> Result<()> {
    let current_time = Clock::get()?.unix_timestamp as u64;
    state.txid_submission_counts.retain(|count| {
        current_time - count.last_updated < SUBMISSION_COUNT_RETENTION_PERIOD
    });
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

fn usize_to_txid_status(index: usize) -> Option<TxidStatus> {
    match index {
        0 => Some(TxidStatus::Invalid),
        1 => Some(TxidStatus::PendingMining),
        2 => Some(TxidStatus::MinedPendingActivation),
        3 => Some(TxidStatus::MinedActivated),
        _ => None,
    }
}

#[event]
pub struct ConsensusReachedEvent {
    pub txid: String,
    pub status: String,
    pub hash: String,
    pub number_of_contributors_included: u32,
}


fn calculate_consensus_and_cleanup(
    program_id: &Pubkey,
    state: &mut OracleContractState, 
    txid: &str
) -> Result<()> {
    let aggregated_data = state.aggregated_consensus_data.iter()
        .find(|data| data.txid == txid)
        .ok_or(OracleError::InvalidTxid)?;

    let current_timestamp = Clock::get()?.unix_timestamp as u64;
    let consensus_status = usize_to_txid_status(
        aggregated_data.status_weights.iter().enumerate().max_by_key(|&(_, &weight)| weight).unwrap_or((0, &0)).0
    ).unwrap_or(TxidStatus::Invalid);

    let consensus_hash = aggregated_data.hash_weights.iter()
        .max_by_key(|&(_, weight)| weight)
        .map(|(hash, _)| hash.clone())
        .unwrap_or_default();

    let mut contributor_count = 0;
    for contributor in &mut state.contributors {
        if contributor.ban_expiry > current_timestamp {
            continue; // Skip banned contributors
        }

        let seeds = &[b"pastel_tx_status_report", txid.as_bytes(), contributor.reward_address.as_ref()];
        let (pda, _bump_seed) = Pubkey::find_program_address(seeds, program_id);
        
        let mut lamports = 0; // Declare lamports as mutable       
        let mut data = vec![];
        let report_account_info = AccountInfo::new(
            &pda,
            false, // is_signer
            true,  // is_writable
            &mut lamports,
            &mut data,
            program_id,
            false, // is_executable
            0      // rent_epoch
        );

        if let Ok(report_account) = Account::<PastelTxStatusReportAccount>::try_from(&report_account_info) {
            let report = &report_account.report;
            let is_accurate_status = report.txid_status == consensus_status;
            let is_accurate_hash = report.first_6_characters_of_sha3_256_hash_of_corresponding_file.as_ref().map_or(false, |hash| hash == &consensus_hash);
            let is_accurate = is_accurate_status && is_accurate_hash;

            if is_accurate || is_accurate_hash {
                contributor_count += 1;
            }

            update_activity_and_compliance_helper(contributor, current_timestamp, is_accurate)?;
        }
    }
    msg!("Consensus Reached for Pastel TXID: {}, Status: {:?}, Hash: {}", txid, consensus_status, consensus_hash);

    emit!(ConsensusReachedEvent {
        txid: txid.to_string(),
        status: format!("{:?}", consensus_status),
        hash: consensus_hash,
        number_of_contributors_included: contributor_count,
    });

    // Call to assess and apply bans
    state.assess_and_apply_bans(current_timestamp);

    // Cleanup logic
    state.contributors.retain(|contributor| {
        current_timestamp - contributor.last_active_timestamp < CONTRIBUTOR_RETENTION_PERIOD
    });
    state.aggregated_consensus_data.retain(|data| data.txid != txid);

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

    pub fn assess_and_apply_bans(&mut self, current_time: u64) {
        for contributor in &mut self.contributors {
            contributor.handle_consensus_failure(current_time);
        }

        self.contributors.retain(|c| !c.calculate_is_banned(current_time));
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
        // Define what makes a contributor reliable
        // For example, a high reliability score which is a ratio of accurate reports to total reports
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
        seeds = [b"pending_payment", txid.as_bytes()],
        bump // You won't explicitly include the bump here; it's handled by Anchor
    )]
    pub pending_payment_account: Account<'info, PendingPaymentAccount>,

    pub system_program: Program<'info, System>,
}


pub fn process_payment_helper(
    ctx: Context<ProcessPayment>, 
    txid: String, 
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
        constraint = oracle_contract_state.admin_pubkey == *admin_account.key @ OracleError::Unauthorized
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

    pub fn add_pending_payment(ctx: Context<HandlePendingPayment>, txid: String, expected_amount: u64, payment_status: PaymentStatus) -> Result<()> {
        let pending_payment = PendingPayment {
            txid: txid.clone(), // Clone the txid for passing to the helper
            expected_amount,
            payment_status,
        };
        add_pending_payment_helper(ctx, txid, pending_payment)
            .map_err(|e| e.into())
    }
    
    pub fn process_payment(ctx: Context<ProcessPayment>, txid: String, amount: u64) -> Result<()> {
        process_payment_helper(ctx, txid, amount)
    }

    pub fn submit_data_report(ctx: Context<SubmitDataReport>, txid: String, report: PastelTxStatusReport) -> Result<()> {
        submit_data_report_helper(ctx, txid, report).map_err(|e| e.into())
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