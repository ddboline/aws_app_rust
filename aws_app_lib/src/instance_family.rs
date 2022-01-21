use anyhow::{format_err, Error};
use std::{fmt, str::FromStr};

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
        f.write_str(s)
    }
}

impl FromStr for InstanceFamilies {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "storage optimized" => Ok(Self::StorageOptimized),
            "accelerated computing" => Ok(Self::AcceleratedComputing),
            "memory optimized" => Ok(Self::MemoryOptimized),
            "compute optimized" => Ok(Self::ComputeOptimized),
            "general purpose" => Ok(Self::GeneralPurpose),
            "micro" => Ok(Self::Micro),
            "gpu optimized" => Ok(Self::GpuOptimized),
            _ => Err(format_err!("Invalid Instance Family")),
        }
    }
}
