use crate::{constant::*, enumeration::TxidStatus, error::*, helper::create_seed, state::*};
use anchor_lang::prelude::*;

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

    if contributor.calculate_is_banned(Clock::get()?.unix_timestamp as u64) {
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
    let weight: f32 = compliance_score + reliability_score;
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

fn find_or_add_common_report_data(
    temp_report_account: &mut TempTxStatusReportAccount,
    common_data: &CommonReportData,
) -> u64 {
    if let Some((index, _)) = temp_report_account
        .common_reports
        .iter()
        .enumerate()
        .find(|(_, data)| *data == common_data)
    {
        index as u64
    } else {
        temp_report_account.common_reports.push(common_data.clone());
        (temp_report_account.common_reports.len() - 1) as u64
    }
}

fn update_submission_count(
    txid_submission_counts_account: &mut Account<TxidSubmissionCountsAccount>,
    txid: &str,
) -> Result<()> {
    // Get the current timestamp
    let current_timestamp_u64 = Clock::get()?.unix_timestamp as u64;

    // Check if the txid already exists in the submission counts
    if let Some(count) = txid_submission_counts_account
        .submission_counts
        .iter_mut()
        .find(|c| c.txid == txid)
    {
        // Update the existing count
        count.count += 1;
        count.last_updated = current_timestamp_u64;
    } else {
        // Insert a new count if the txid does not exist
        txid_submission_counts_account
            .submission_counts
            .push(TxidSubmissionCount {
                txid: txid.to_string(),
                count: 1,
                last_updated: current_timestamp_u64,
            });
    }

    Ok(())
}

fn aggregate_consensus_data(
    aggregated_data_account: &mut Account<AggregatedConsensusDataAccount>,
    report: &PastelTxStatusReport,
    weight: f32,
    txid: &str,
) -> Result<()> {
    let scaled_weight = (weight * 100.0) as i32; // Scaling by a factor of 100
    let current_timestamp = Clock::get()?.unix_timestamp as u64;

    // Check if the txid already exists in the aggregated consensus data
    if let Some(data_entry) = aggregated_data_account
        .consensus_data
        .iter_mut()
        .find(|d| d.txid == txid)
    {
        // Update existing data
        data_entry.status_weights[report.txid_status as usize] += scaled_weight;
        if let Some(hash) = &report.first_6_characters_of_sha3_256_hash_of_corresponding_file {
            update_hash_weight(&mut data_entry.hash_weights, hash, scaled_weight);
        }
        data_entry.last_updated = current_timestamp;
        // Handling the Option<String> here
        data_entry.first_6_characters_of_sha3_256_hash_of_corresponding_file = report
            .first_6_characters_of_sha3_256_hash_of_corresponding_file
            .clone()
            .unwrap_or_default();
    } else {
        // Create new data
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
        aggregated_data_account.consensus_data.push(new_data);
    }

    Ok(())
}

pub fn should_calculate_consensus(
    txid_submission_counts_account: &Account<TxidSubmissionCountsAccount>,
    txid: &str,
) -> Result<bool> {
    // Retrieve the count of submissions and last updated timestamp for the given txid
    let (submission_count, last_updated) = txid_submission_counts_account
        .submission_counts
        .iter()
        .find(|c| c.txid == txid)
        .map(|c| (c.count, c.last_updated))
        .unwrap_or((0, 0));

    // Check if the minimum threshold of reports is met
    let min_threshold_met = submission_count >= MIN_NUMBER_OF_ORACLES as u32;

    // Get the current unix timestamp from the Solana clock
    let current_unix_timestamp = Clock::get()?.unix_timestamp as u64;

    // Check if N minutes have elapsed since the last update
    let max_waiting_period_elapsed_for_txid = current_unix_timestamp - last_updated
        >= MAX_DURATION_IN_SECONDS_FROM_LAST_REPORT_SUBMISSION_BEFORE_COMPUTING_CONSENSUS;

    // Calculate consensus if minimum threshold is met or if N minutes have passed with at least MIN_NUMBER_OF_ORACLES reports
    Ok(min_threshold_met
        || (max_waiting_period_elapsed_for_txid
            && submission_count >= MIN_NUMBER_OF_ORACLES as u32))
}

fn calculate_consensus(
    aggregated_data_account: &Account<AggregatedConsensusDataAccount>,
    temp_report_account: &TempTxStatusReportAccount,
    contributor_data_account: &mut Account<ContributorDataAccount>,
    txid: &str,
) -> Result<()> {
    let current_timestamp = Clock::get()?.unix_timestamp as u64;
    let (consensus_status, consensus_hash) = get_aggregated_data(aggregated_data_account, txid)
        .map(|data| compute_consensus(data))
        .unwrap_or((TxidStatus::Invalid, String::new()));

    let mut updated_contributors = Vec::new();
    let mut contributor_count = 0;

    for temp_report in temp_report_account.reports.iter() {
        let common_data = &temp_report_account.common_reports[temp_report.common_data_ref as usize];
        let specific_data = &temp_report.specific_data;

        if common_data.txid == txid
            && !updated_contributors.contains(&specific_data.contributor_reward_address)
        {
            if let Some(contributor) = contributor_data_account
                .contributors
                .iter_mut()
                .find(|c| c.reward_address == specific_data.contributor_reward_address)
            {
                let is_accurate = common_data.txid_status == consensus_status
                    && common_data
                        .first_6_characters_of_sha3_256_hash_of_corresponding_file
                        .as_ref()
                        .map_or(false, |hash| hash == &consensus_hash);
                update_contributor(contributor, current_timestamp, is_accurate);
                updated_contributors.push(specific_data.contributor_reward_address);
            }
            contributor_count += 1;
        }
    }
    msg!("Consensus reached for TXID: {}, Status: {:?}, Hash: {}, Number of Contributors Included: {}", txid, consensus_status, consensus_hash, contributor_count);

    Ok(())
}

fn post_consensus_tasks(
    txid_submission_counts_account: &mut Account<TxidSubmissionCountsAccount>,
    aggregated_data_account: &mut Account<AggregatedConsensusDataAccount>,
    temp_report_account: &mut TempTxStatusReportAccount,
    contributor_data_account: &mut Account<ContributorDataAccount>,
    txid: &str,
) -> Result<()> {
    let current_timestamp = Clock::get()?.unix_timestamp as u64;

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

// Function to update hash weight
fn update_hash_weight(hash_weights: &mut Vec<HashWeight>, hash: &str, weight: i32) {
    let mut found = false;

    for hash_weight in hash_weights.iter_mut() {
        if hash_weight.hash.as_str() == hash {
            hash_weight.weight += weight;
            found = true;
            break;
        }
    }

    if !found {
        hash_weights.push(HashWeight {
            hash: hash.to_string(), // Clone only when necessary
            weight,
        });
    }
}

fn get_aggregated_data<'a>(
    aggregated_data_account: &'a Account<AggregatedConsensusDataAccount>,
    txid: &str,
) -> Option<&'a AggregatedConsensusData> {
    aggregated_data_account
        .consensus_data
        .iter()
        .find(|data| data.txid == txid)
}

fn compute_consensus(aggregated_data: &AggregatedConsensusData) -> (TxidStatus, String) {
    let consensus_status = aggregated_data
        .status_weights
        .iter()
        .enumerate()
        .max_by_key(|&(_, weight)| weight)
        .map(|(index, _)| usize_to_txid_status(index).unwrap_or(TxidStatus::Invalid))
        .unwrap();

    let consensus_hash = aggregated_data
        .hash_weights
        .iter()
        .max_by_key(|hash_weight| hash_weight.weight)
        .map(|hash_weight| hash_weight.hash.clone())
        .unwrap_or_default();

    (consensus_status, consensus_hash)
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

fn update_contributor(contributor: &mut Contributor, current_timestamp: u64, is_accurate: bool) {
    // Check if the contributor is banned before proceeding. If so, just return.
    if contributor.calculate_is_banned(current_timestamp) {
        msg!(
            "Contributor is currently banned and cannot be updated: {}",
            contributor.reward_address
        );
        return; // We don't stop the process here, just skip this contributor.
    }

    // Updating scores
    update_scores(contributor, current_timestamp, is_accurate);

    // Applying bans based on report accuracy
    apply_bans(contributor, current_timestamp, is_accurate);

    // Updating contributor statuses
    update_statuses(contributor, current_timestamp);
}

fn apply_bans(contributor: &mut Contributor, current_timestamp: u64, is_accurate: bool) {
    if !is_accurate {
        if contributor.total_reports_submitted <= CONTRIBUTIONS_FOR_TEMPORARY_BAN
            && contributor.consensus_failures % TEMPORARY_BAN_THRESHOLD == 0
        {
            contributor.ban_expiry = current_timestamp + TEMPORARY_BAN_DURATION;
            msg!("Contributor: {} is temporarily banned as of {} because they have submitted {} reports and have {} consensus failures, more than the maximum allowed consensus failures of {}. Ban expires on: {}", 
            contributor.reward_address, current_timestamp, contributor.total_reports_submitted, contributor.consensus_failures, TEMPORARY_BAN_THRESHOLD, contributor.ban_expiry);
        } else if contributor.total_reports_submitted >= CONTRIBUTIONS_FOR_PERMANENT_BAN
            && contributor.consensus_failures >= PERMANENT_BAN_THRESHOLD
        {
            contributor.ban_expiry = u64::MAX;
            msg!("Contributor: {} is permanently banned as of {} because they have submitted {} reports and have {} consensus failures, more than the maximum allowed consensus failures of {}. Removing from list of contributors!", 
            contributor.reward_address, current_timestamp, contributor.total_reports_submitted, contributor.consensus_failures, PERMANENT_BAN_THRESHOLD);
        }
    }
}

fn update_scores(contributor: &mut Contributor, current_timestamp: u64, is_accurate: bool) {
    let time_diff = current_timestamp.saturating_sub(contributor.last_active_timestamp);
    let hours_inactive: f32 = time_diff as f32 / 3_600.0;

    // Dynamic scaling for accuracy
    let accuracy_scaling = if is_accurate {
        (1.0 + contributor.current_streak as f32 * 0.1).min(2.0) // Increasing bonus for consecutive accuracy
    } else {
        1.0
    };

    let time_weight = 1.0 / (1.0 + hours_inactive / 480.0);

    let base_score_increment = 20.0; // Adjusted base increment for a more gradual increase

    let score_increment = base_score_increment * accuracy_scaling * time_weight;

    // Exponential penalty for inaccuracies
    let score_decrement = 20.0 * (1.0 + contributor.consensus_failures as f32 * 0.5).min(3.0);

    let decay_rate: f32 = 0.99; // Adjusted decay rate
    let decay_factor = decay_rate.powf(hours_inactive / 24.0);

    let streak_bonus = if is_accurate {
        (contributor.current_streak as f32 / 10.0).min(3.0).max(0.0) // Enhanced streak bonus
    } else {
        0.0
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
        contributor.compliance_score = (contributor.compliance_score - score_decrement).max(0.0);
    }

    contributor.compliance_score *= decay_factor;

    // Integrating reliability score into compliance score calculation
    let reliability_factor = (contributor.accurate_reports_count as f32
        / contributor.total_reports_submitted as f32)
        .clamp(0.0, 1.0);
    contributor.compliance_score = (contributor.compliance_score * reliability_factor).min(100.0);

    contributor.compliance_score = logistic_scale(contributor.compliance_score, 100.0, 0.1, 50.0); // Adjusted logistic scaling

    contributor.reliability_score = reliability_factor * 100.0;

    log_score_updates(contributor);
}

fn logistic_scale(score: f32, max_value: f32, steepness: f32, midpoint: f32) -> f32 {
    max_value / (1.0 + (-steepness * (score - midpoint)).exp())
}

fn log_score_updates(contributor: &Contributor) {
    msg!(
        "Scores After Update: Address: {}, Compliance Score: {}, Reliability Score: {}",
        contributor.reward_address,
        contributor.compliance_score,
        contributor.reliability_score
    );
}

pub fn apply_permanent_bans(contributor_data_account: &mut Account<ContributorDataAccount>) {
    // Collect addresses of contributors to be removed for efficient logging
    let contributors_to_remove: Vec<String> = contributor_data_account
        .contributors
        .iter()
        .filter(|c| c.ban_expiry == u64::MAX)
        .map(|c| c.reward_address.to_string()) // Convert Pubkey to String
        .collect();

    // Log information about the removal process
    msg!("Now removing permanently banned contributors! Total number of contributors before removal: {}, Number of contributors to be removed: {}, Addresses of contributors to be removed: {:?}",
        contributor_data_account.contributors.len(), contributors_to_remove.len(), contributors_to_remove);

    // Retain only contributors who are not permanently banned
    contributor_data_account
        .contributors
        .retain(|c| c.ban_expiry != u64::MAX);
}

fn update_statuses(contributor: &mut Contributor, current_timestamp: u64) {
    // Updating recently active status
    let recent_activity_threshold = 86_400; // 24 hours in seconds
    contributor.is_recently_active =
        current_timestamp - contributor.last_active_timestamp < recent_activity_threshold;

    // Updating reliability status
    contributor.is_reliable = if contributor.total_reports_submitted > 0 {
        let reliability_ratio =
            contributor.accurate_reports_count as f32 / contributor.total_reports_submitted as f32;
        reliability_ratio >= 0.8 // Example threshold for reliability
    } else {
        false
    };

    // Updating eligibility for rewards
    contributor.is_eligible_for_rewards = contributor.total_reports_submitted
        >= MIN_REPORTS_FOR_REWARD
        && contributor.reliability_score >= MIN_RELIABILITY_SCORE_FOR_REWARD
        && contributor.compliance_score >= MIN_COMPLIANCE_SCORE_FOR_REWARD;
}
