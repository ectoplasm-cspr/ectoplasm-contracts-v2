use casper_types::ApiError;

/// Errors for CEP-18 token contract
#[repr(u16)]
pub enum Cep18Error {
    InsufficientBalance = 1,
    InsufficientAllowance = 2,
    Unauthorized = 3,
    Overflow = 4,
    Underflow = 5,
}

impl From<Cep18Error> for ApiError {
    fn from(error: Cep18Error) -> Self {
        ApiError::User(error as u16)
    }
}
