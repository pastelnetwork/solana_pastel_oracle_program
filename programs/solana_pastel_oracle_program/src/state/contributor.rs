use crate::constant::{
    MIN_COMPLIANCE_SCORE_FOR_REWARD, MIN_RELIABILITY_SCORE_FOR_REWARD, MIN_REPORTS_FOR_REWARD,
};
use anchor_lang::prelude::*;

#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct Contributor {
    pub reward_address: Pubkey,
    pub registration_entrance_fee_transaction_signature: String,
    pub compliance_score: f32,
    pub last_active_timestamp: u64,
    pub total_reports_submitted: u32,
    pub accurate_reports_count: u32,
    pub current_streak: u32,
    pub reliability_score: f32,
    pub consensus_failures: u32,
    pub ban_expiry: u64,
    pub is_eligible_for_rewards: bool,
    pub is_recently_active: bool,
    pub is_reliable: bool,
}

#[account]
pub struct ContributorDataAccount {
    pub contributors: Vec<Contributor>,
}

impl Contributor {
    // Check if the contributor is currently banned
    pub fn calculate_is_banned(&self, current_time: u64) -> bool {
        current_time < self.ban_expiry
    }

    // Method to determine if the contributor is eligible for rewards
    pub fn calculate_is_eligible_for_rewards(&self) -> bool {
        self.total_reports_submitted >= MIN_REPORTS_FOR_REWARD
            && self.reliability_score >= MIN_RELIABILITY_SCORE_FOR_REWARD
            && self.compliance_score >= MIN_COMPLIANCE_SCORE_FOR_REWARD
    }
}
