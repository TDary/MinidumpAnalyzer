use crate::Verbosity;
use crate::analyzer::CrashReport;
use anyhow::Result;
use std::path::Path;

#[allow(clippy::too_many_arguments)]
pub async fn analyze(
    _dmp_path: &str,
    _symbols_dir: &Path,
    _cache_dir: &Path,
    _pdb_dir: Option<&Path>,
    _download_only: bool,
    _include_all_threads: bool,
    _include_registers: bool,
    _verbosity: Verbosity,
) -> Result<CrashReport> {
    anyhow::bail!("Android 渠道解析尚未实现")
}
