use thiserror::Error;

#[derive(Error, Debug)]
pub enum InjectorError {
    #[error("Process not found: {0}")]
    ProcessNotFound(String),
    #[error("Memory allocation failed: 0x{0:X}")]
    MemoryAllocFailed(u32),
    #[error("PE parse failed: {0}")]
    PeParseFailed(String),
    #[error("DLL load failed: {0}")]
    DllLoadFailed(String),
    #[error("Export not found: {0}")]
    ExportNotFound(String),
    #[error("Timeout")]
    Timeout,
    #[error("Insufficient privileges")]
    InsufficientPrivileges,
    #[error("Architecture mismatch: {0}")]
    ArchMismatch(String),
    #[error("Communication error: {0}")]
    CommunicationError(String),
    #[error("Unknown error: {0}")]
    Unknown(String),
}