pub mod analyzer;
pub mod symbols;

use anyhow::Result;
use minidump::{
    Minidump, MinidumpException, MinidumpMiscInfo, MinidumpModuleList, MinidumpSystemInfo,
};
use minidump_processor::{http_symbol_supplier, MultiSymbolProvider, Symbolizer};
use std::path::Path;

const MICROSOFT_SYMBOL_SERVER: &str = "https://msdl.microsoft.com/download/symbols";

pub async fn analyze(
    dmp_path: &str,
    symbols_dir: &Path,
    cache_dir: &Path,
    pdb_dir: Option<&Path>,
    download_only: bool,
    include_all_threads: bool,
    include_registers: bool,
    quiet: bool,
) -> Result<analyzer::CrashReport> {
    let dump = Minidump::read_path(dmp_path)?;

    let sys_info = dump.get_stream::<MinidumpSystemInfo>().ok();
    let modules = dump.get_stream::<MinidumpModuleList>().ok();
    let exception = dump.get_stream::<MinidumpException>().ok();
    let misc_info = dump.get_stream::<MinidumpMiscInfo>().ok();

    let context = if include_registers {
        exception
            .as_ref()
            .and_then(|exc| sys_info.as_ref().and_then(|si| exc.context(si, misc_info.as_ref())))
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
                quiet,
            )
            .await?;
        }
        if download_only {
            // Return empty report — caller only wanted symbols
            return Ok(analyzer::CrashReport {
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

    let supplier =
        http_symbol_supplier(symbol_paths, symbol_urls, symbols_cache, symbols_tmp, timeout);
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
    );

    Ok(report)
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
        // Doesn't exist yet
        assert!(!symbols::sym_exists(&tmp, pdb, id));

        // Create it
        let sym_dir = tmp.join(pdb).join(id);
        std::fs::create_dir_all(&sym_dir).unwrap();
        std::fs::write(sym_dir.join("test.sym"), b"MODULE windows x86 ABC123 test").unwrap();
        assert!(symbols::sym_exists(&tmp, pdb, id));

        // Clean up
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
