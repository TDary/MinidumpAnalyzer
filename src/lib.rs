pub mod analyzer;
pub mod channel;
pub mod symbols;

use anyhow::Result;
use serde::Serialize;
use std::path::Path;
use std::str::FromStr;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub enum Channel {
    #[serde(rename = "pc")]
    Pc,
    #[serde(rename = "android")]
    Android,
    #[serde(rename = "ios")]
    Ios,
}

impl FromStr for Channel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pc" | "windows" | "win" => Ok(Self::Pc),
            "android" | "aos" => Ok(Self::Android),
            "ios" => Ok(Self::Ios),
            _ => Err(format!(
                "不支持的渠道: \"{s}\"，可选: pc (windows/win), android (aos), ios"
            )),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

impl Verbosity {
    pub fn is_silent(self) -> bool {
        matches!(self, Self::Quiet)
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn analyze(
    dmp_path: &str,
    symbols_dir: &Path,
    cache_dir: &Path,
    pdb_dir: Option<&Path>,
    download_only: bool,
    include_all_threads: bool,
    include_registers: bool,
    verbosity: Verbosity,
    channel: Channel,
) -> Result<analyzer::CrashReport> {
    channel::analyze_by_channel(
        dmp_path,
        symbols_dir,
        cache_dir,
        pdb_dir,
        download_only,
        include_all_threads,
        include_registers,
        verbosity,
        channel,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guid_to_breakpad_id() {
        assert_eq!(
            symbols::guid_to_breakpad_id("a1b2c3d4-e5f6-7890-abcd-ef1234567890"),
            "A1B2C3D4E5F67890ABCDEF1234567890"
        );
    }

    #[test]
    fn test_exception_reason_known() {
        assert_eq!(analyzer::exception_reason(0xC0000005), "ACCESS_VIOLATION");
        assert_eq!(analyzer::exception_reason(0xC00000FD), "STACK_OVERFLOW");
        assert_eq!(analyzer::exception_reason(0x80000003), "BREAKPOINT");
    }

    #[test]
    fn test_exception_reason_unknown() {
        assert_eq!(analyzer::exception_reason(0xDEADBEEF), "UNKNOWN");
    }

    #[test]
    fn test_sym_exists() {
        let tmp = std::env::temp_dir().join("test_sym_cache");
        let pdb = "test.pdb";
        let id = "ABC123";
        assert!(!symbols::sym_exists(&tmp, pdb, id));

        let sym_dir = tmp.join(pdb).join(id);
        std::fs::create_dir_all(&sym_dir).unwrap();
        std::fs::write(sym_dir.join("test.sym"), b"MODULE windows x86 ABC123 test").unwrap();
        assert!(symbols::sym_exists(&tmp, pdb, id));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
