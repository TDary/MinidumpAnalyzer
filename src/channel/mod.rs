pub mod android;
pub mod ios;
pub mod pc;

use crate::analyzer::CrashReport;
use crate::{Channel, Verbosity};
use anyhow::Result;
use std::path::Path;

#[allow(clippy::too_many_arguments)]
pub async fn analyze_by_channel(
    dmp_path: &str,
    symbols_dir: &Path,
    cache_dir: &Path,
    pdb_dir: Option<&Path>,
    download_only: bool,
    include_all_threads: bool,
    include_registers: bool,
    verbosity: Verbosity,
    channel: Channel,
) -> Result<CrashReport> {
    match channel {
        Channel::Pc => {
            pc::analyze(
                dmp_path,
                symbols_dir,
                cache_dir,
                pdb_dir,
                download_only,
                include_all_threads,
                include_registers,
                verbosity,
            )
            .await
        }
        Channel::Android => {
            android::analyze(
                dmp_path,
                symbols_dir,
                cache_dir,
                pdb_dir,
                download_only,
                include_all_threads,
                include_registers,
                verbosity,
            )
            .await
        }
        Channel::Ios => {
            ios::analyze(
                dmp_path,
                symbols_dir,
                cache_dir,
                pdb_dir,
                download_only,
                include_all_threads,
                include_registers,
                verbosity,
            )
            .await
        }
    }
}
