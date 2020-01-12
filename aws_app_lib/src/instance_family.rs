use anyhow::{format_err, Error};
use std::fmt;
use std::str::FromStr;

#[derive(Debug)]
pub enum InstanceFamilies {
    StorageOptimized,
    AcceleratedComputing,
    MemoryOptimized,
    ComputeOptimized,
    GeneralPurpose,
    Micro,
    GpuOptimized,
}

impl fmt::Display for InstanceFamilies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::StorageOptimized => "Storage Optimized",
            Self::AcceleratedComputing => "Accelerated Computing",
            Self::MemoryOptimized => "Memory Optimized",
            Self::ComputeOptimized => "Compute Optimized",
            Self::GeneralPurpose => "General Purpose",
            Self::Micro => "Micro",
            Self::GpuOptimized => "GPU Optimized",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for InstanceFamilies {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Storage Optimized" => Ok(Self::StorageOptimized),
            "Accelerated Computing" => Ok(Self::AcceleratedComputing),
            "Memory optimized" => Ok(Self::MemoryOptimized),
            "Compute optimized" => Ok(Self::ComputeOptimized),
            "General Purpose" => Ok(Self::GeneralPurpose),
            "Micro" => Ok(Self::Micro),
            "Storage optimized" => Ok(Self::StorageOptimized),
            "Memory Optimized" => Ok(Self::MemoryOptimized),
            "General purpose" => Ok(Self::GeneralPurpose),
            "GPU optimized" => Ok(Self::GpuOptimized),
            "Compute Optimized" => Ok(Self::ComputeOptimized),
            _ => Err(format_err!("Invalid Instance Family")),
        }
    }
}
