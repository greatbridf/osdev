use crate::kernel::constants::{EFAULT, EINVAL, ENOENT};
use acpi::AcpiError;

#[derive(Debug)]
pub struct PciError(u32);

impl From<AcpiError> for PciError {
    fn from(error: AcpiError) -> Self {
        match error {
            AcpiError::NoValidRsdp => PciError(ENOENT),
            AcpiError::RsdpIncorrectSignature => PciError(EINVAL),
            AcpiError::RsdpInvalidOemId => PciError(EINVAL),
            AcpiError::RsdpInvalidChecksum => PciError(EINVAL),
            AcpiError::SdtInvalidSignature(_) => PciError(EINVAL),
            AcpiError::SdtInvalidOemId(_) => PciError(EINVAL),
            AcpiError::SdtInvalidTableId(_) => PciError(EINVAL),
            AcpiError::SdtInvalidChecksum(_) => PciError(EINVAL),
            AcpiError::TableMissing(_) => PciError(ENOENT),
            AcpiError::InvalidFacsAddress => PciError(EFAULT),
            AcpiError::InvalidDsdtAddress => PciError(EFAULT),
            AcpiError::InvalidMadt(_) => PciError(EINVAL),
            AcpiError::InvalidGenericAddress => PciError(EFAULT),
            AcpiError::AllocError => PciError(EFAULT),
        }
    }
}

impl From<PciError> for u32 {
    fn from(value: PciError) -> Self {
        value.0
    }
}

impl From<u32> for PciError {
    fn from(value: u32) -> Self {
        Self(value)
    }
}
