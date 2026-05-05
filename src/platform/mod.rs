mod os;
pub mod packages;

pub use os::{Arch, Distro, Os};
pub use packages::PackageManager;

/// Detected platform information.
#[derive(Debug, Clone)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
    pub distro: Option<Distro>,
    pub managers: Vec<PackageManager>,
}

/// Errors that can occur during platform detection.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("unsupported operating system")]
    UnsupportedOs,
}

/// Detect the current platform.
pub fn detect() -> Result<Platform, Error> {
    let os = os::detect_os()?;
    let arch = os::detect_arch();
    let distro = if os == Os::Linux {
        os::detect_distro()
    } else {
        None
    };

    let managers = packages::discover();

    Ok(Platform { os, arch, distro, managers })
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.os, self.arch)?;
        if let Some(ref distro) = self.distro {
            write!(f, ", {distro}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_valid_platform() {
        let platform = detect().expect("should detect current platform");
        // We're running on a real machine, so these should be populated
        match platform.os {
            Os::Linux | Os::MacOs | Os::Windows => {}
        }
        match platform.arch {
            Arch::X86_64 | Arch::Aarch64 | Arch::Other(_) => {}
        }
    }
}
