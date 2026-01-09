use casper_types::ApiError;

#[repr(u16)]
pub enum BondingCurveError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InsufficientPayment = 4,
    InsufficientTokens = 5,
    InsufficientLiquidity = 6,
    CurveNotActive = 7,
    CurveAlreadyGraduated = 8,
    GraduationThresholdNotMet = 9,
    RefundNotAvailable = 10,
    DeadlineNotReached = 11,
    NoRefundAvailable = 12,
    InvalidCurveType = 13,
    InvalidAmount = 14,
    TransferFailed = 15,
    MilestoneNotUnlocked = 16,
    NoPromoToWithdraw = 17,
    Overflow = 18,
    DivisionByZero = 19,
    LockedReentrancy = 20,
}

impl From<BondingCurveError> for ApiError {
    fn from(error: BondingCurveError) -> Self {
        ApiError::User(error as u16)
    }
}
