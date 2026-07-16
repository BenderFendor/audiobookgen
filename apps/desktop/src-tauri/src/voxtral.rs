//! Hardware detection for the in-process Voxtral HQQ INT4 worker.
//!
//! The Python narration worker owns model loading and serialized inference.
//! This module keeps the cheap NVIDIA probe in Rust so the Models screen can
//! explain compatibility before downloading several gigabytes of weights.

const MIN_COMPUTE_CAPABILITY: f32 = 8.0;
const TARGET_VRAM_MIB: u64 = 12 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct NvidiaGpu {
    pub name: String,
    pub memory_mib: u64,
    pub free_memory_mib: u64,
    pub compute_capability: f32,
    pub driver_version: String,
}

impl NvidiaGpu {
    pub fn support_message(&self) -> String {
        if self.compute_capability < MIN_COMPUTE_CAPABILITY {
            return format!(
                "{} has CUDA compute capability {:.1}; selective Voxtral INT4 requires 8.0 or newer.",
                self.name, self.compute_capability
            );
        }
        if self.memory_mib < TARGET_VRAM_MIB {
            return format!(
                "{} has {:.1} GB VRAM. The INT4 profile targets 12 GB; this device is not supported without measured evidence.",
                self.name,
                self.memory_mib as f64 / 1024.0
            );
        }
        format!(
            "{} is compatible with the 12 GB selective INT4 profile ({:.1} GB total, {:.1} GB currently free, compute {:.1}, driver {}).",
            self.name,
            self.memory_mib as f64 / 1024.0,
            self.free_memory_mib as f64 / 1024.0,
            self.compute_capability,
            self.driver_version
        )
    }

    pub fn can_run_voxtral(&self) -> bool {
        self.compute_capability >= MIN_COMPUTE_CAPABILITY && self.memory_mib >= TARGET_VRAM_MIB
    }
}

fn parse_nvidia_smi_line(line: &str) -> Option<NvidiaGpu> {
    let mut fields = line.split(',').map(str::trim);
    Some(NvidiaGpu {
        name: fields.next()?.to_owned(),
        memory_mib: fields.next()?.parse().ok()?,
        free_memory_mib: fields.next()?.parse().ok()?,
        compute_capability: fields.next()?.parse().ok()?,
        driver_version: fields.next()?.to_owned(),
    })
}

pub fn detect_nvidia_gpu() -> Option<NvidiaGpu> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,memory.free,compute_cap,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_nvidia_smi_line)
        .max_by_key(|gpu| gpu.memory_mib)
}

pub fn cuda_status_message() -> String {
    if !cfg!(target_os = "linux") {
        return "Voxtral INT4 currently requires Linux and an NVIDIA CUDA GPU.".into();
    }
    detect_nvidia_gpu().map_or_else(
        || "No NVIDIA CUDA GPU was detected; Voxtral INT4 cannot run.".into(),
        |gpu| gpu.support_message(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_a_12_gb_ampere_gpu() {
        let gpu = parse_nvidia_smi_line("NVIDIA GeForce RTX 3060, 12288, 9000, 8.6, 595.71.05")
            .expect("valid nvidia-smi row");
        assert!(gpu.can_run_voxtral());
        assert!(gpu.support_message().contains("selective INT4"));
    }

    #[test]
    fn rejects_old_compute_capability() {
        let gpu = NvidiaGpu {
            name: "Older GPU".into(),
            memory_mib: 24_576,
            free_memory_mib: 20_000,
            compute_capability: 7.5,
            driver_version: "595.71.05".into(),
        };
        assert!(!gpu.can_run_voxtral());
        assert!(gpu.support_message().contains("8.0 or newer"));
    }

    #[test]
    fn does_not_claim_sub_12_gb_cards_are_supported() {
        let gpu = NvidiaGpu {
            name: "RTX 3070".into(),
            memory_mib: 8_192,
            free_memory_mib: 7_000,
            compute_capability: 8.6,
            driver_version: "595.71.05".into(),
        };
        assert!(!gpu.can_run_voxtral());
        assert!(gpu.support_message().contains("targets 12 GB"));
    }
}
