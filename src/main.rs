mod analyzer;
mod symbols;

use anyhow::Result;
use clap::Parser;
use minidump::{
    Minidump, MinidumpException, MinidumpMiscInfo, MinidumpModuleList, MinidumpSystemInfo,
};
use minidump_processor::{http_symbol_supplier, MultiSymbolProvider, Symbolizer};
use std::path::PathBuf;

const MICROSOFT_SYMBOL_SERVER: &str = "https://msdl.microsoft.com/download/symbols";

#[derive(Parser)]
#[command(name = "minidump-analyzer", about = "Windows Minidump 崩溃堆栈解析工具")]
struct Cli {
    /// Minidump (.dmp) 文件路径
    dmp_path: String,

    /// 本地 .sym 符号文件目录
    #[arg(short = 's', long, default_value = "./symbols")]
    symbols_dir: PathBuf,

    /// 符号缓存目录
    #[arg(short = 'c', long, default_value = "./sym_cache")]
    cache_dir: PathBuf,

    /// 本地 PDB 文件目录，自动用 dump_syms 转换
    #[arg(short = 'p', long)]
    pdb_dir: Option<PathBuf>,

    /// 仅下载/转换缺失符号，不解析 dmp
    #[arg(long)]
    download_symbols: bool,

    /// 输出所有线程的调用栈
    #[arg(long)]
    all_threads: bool,

    /// 输出崩溃线程的寄存器上下文
    #[arg(long)]
    registers: bool,

    /// 等价于 --all-threads --registers
    #[arg(long)]
    full: bool,

    /// 以 JSON 格式输出分析结果
    #[arg(long)]
    json: bool,

    /// 将报告写入文件（文本或 JSON），不指定则输出到 stdout
    #[arg(short = 'o', long)]
    output: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    symbols::check_dump_syms()?;

    let show_all_threads = cli.full || cli.all_threads;
    let show_registers = cli.full || cli.registers;

    eprintln!("正在解析 Minidump: {}", cli.dmp_path);

    let dump = Minidump::read_path(&cli.dmp_path)?;

    let sys_info = dump.get_stream::<MinidumpSystemInfo>().ok();
    let modules = dump.get_stream::<MinidumpModuleList>().ok();
    let exception = dump.get_stream::<MinidumpException>().ok();
    let misc_info = dump.get_stream::<MinidumpMiscInfo>().ok();

    let context = if show_registers {
        exception
            .as_ref()
            .and_then(|exc| sys_info.as_ref().and_then(|si| exc.context(si, misc_info.as_ref())))
            .map(|cow| cow.into_owned())
    } else {
        None
    };

    // Symbol prefetch: local PDB conversion + optional Microsoft download
    if cli.download_symbols || cli.pdb_dir.is_some() {
        eprintln!("\n--- 获取缺失符号 ---");
        if let Some(ref mods) = modules {
            symbols::download_missing_symbols(
                mods,
                &cli.symbols_dir,
                &cli.cache_dir,
                cli.pdb_dir.as_deref(),
                cli.download_symbols,
            )
            .await?;
        }
        if cli.download_symbols {
            return Ok(());
        }
    }

    // Symbol resolution
    let symbol_paths = vec![cli.symbols_dir.clone(), cli.cache_dir.clone()];
    let symbol_urls = vec![MICROSOFT_SYMBOL_SERVER.to_string()];
    let symbols_cache = cli.cache_dir.clone();
    let symbols_tmp = std::env::temp_dir();
    let timeout = std::time::Duration::from_secs(120);

    let supplier =
        http_symbol_supplier(symbol_paths, symbol_urls, symbols_cache, symbols_tmp, timeout);
    let symbolizer = Symbolizer::new(supplier);

    let mut provider = MultiSymbolProvider::new();
    provider.add(Box::new(symbolizer));

    let state = minidump_processor::process_minidump(&dump, &provider).await?;

    // Build report
    let report = analyzer::build_report(
        sys_info,
        exception,
        modules,
        context,
        &state,
        &cli.symbols_dir,
        &cli.cache_dir,
        show_all_threads,
        show_registers,
    );

    // Output
    let output = if cli.json {
        serde_json::to_string_pretty(&report)?
    } else {
        analyzer::format_text(&report, show_all_threads, show_registers)
    };

    if let Some(ref path) = cli.output {
        std::fs::write(path, &output)?;
        eprintln!("报告已保存: {}", path.display());
    } else {
        print!("{output}");
    }

    Ok(())
}
