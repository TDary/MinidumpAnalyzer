use crate::Verbosity;
use crate::analyzer::{self, CrashReport};
use crate::symbols;
use anyhow::Result;
use minidump::{
    Minidump, MinidumpException, MinidumpMiscInfo, MinidumpModuleList, MinidumpSystemInfo,
};
use minidump_processor::{MultiSymbolProvider, Symbolizer, http_symbol_supplier};
use std::path::Path;

const MICROSOFT_SYMBOL_SERVER: &str = "https://msdl.microsoft.com/download/symbols";

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
) -> Result<CrashReport> {
    let dump = Minidump::read_path(dmp_path)?;

    let sys_info = dump.get_stream::<MinidumpSystemInfo>().ok();
    let modules = dump.get_stream::<MinidumpModuleList>().ok();
    let exception = dump.get_stream::<MinidumpException>().ok();
    let misc_info = dump.get_stream::<MinidumpMiscInfo>().ok();

    let context = if include_registers {
        exception
            .as_ref()
            .and_then(|exc| {
                sys_info
                    .as_ref()
                    .and_then(|si| exc.context(si, misc_info.as_ref()))
            })
            .map(|cow| cow.into_owned())
    } else {
        None
    };

    // Symbol prefetch
    if download_only || pdb_dir.is_some() {
        if let Some(ref mods) = modules {
            symbols::download_missing_symbols(
                mods,
                symbols_dir,
                cache_dir,
                pdb_dir,
                download_only && pdb_dir.is_none(),
                verbosity,
            )
            .await?;
        }
        if download_only {
            return Ok(CrashReport {
                channel: crate::Channel::Pc,
                system_info: None,
                exception: None,
                modules: vec![],
                threads: vec![],
            });
        }
    }

    // Symbol resolution
    let symbol_paths = vec![symbols_dir.to_path_buf(), cache_dir.to_path_buf()];
    let symbol_urls = vec![MICROSOFT_SYMBOL_SERVER.to_string()];
    let symbols_cache = cache_dir.to_path_buf();
    let symbols_tmp = std::env::temp_dir();
    let timeout = std::time::Duration::from_secs(120);

    let supplier = http_symbol_supplier(
        symbol_paths,
        symbol_urls,
        symbols_cache,
        symbols_tmp,
        timeout,
    );
    let symbolizer = Symbolizer::new(supplier);

    let mut provider = MultiSymbolProvider::new();
    provider.add(Box::new(symbolizer));

    let state = minidump_processor::process_minidump(&dump, &provider).await?;

    let report = analyzer::build_report(
        sys_info,
        exception,
        modules,
        context,
        &state,
        symbols_dir,
        cache_dir,
        include_all_threads,
        include_registers,
        crate::Channel::Pc,
    );

    Ok(report)
}
