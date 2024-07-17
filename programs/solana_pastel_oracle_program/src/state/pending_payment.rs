use crate::enumeration::PaymentStatus;
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub struct PendingPayment {
    pub txid: String,
    pub expected_amount: u64,
    pub payment_status: PaymentStatus,
}

#[account]
pub struct PendingPaymentAccount {
    pub pending_payment: PendingPayment,
}
