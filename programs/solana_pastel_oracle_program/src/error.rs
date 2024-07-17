use anchor_lang::prelude::*;

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
}

impl From<OracleError> for ProgramError {
    fn from(e: OracleError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
