use anyhow::Result;
use clap::Parser;
use minidump_analyzer::{analyze, symbols};
use std::path::PathBuf;

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

    let report = analyze(
        &cli.dmp_path,
        &cli.symbols_dir,
        &cli.cache_dir,
        cli.pdb_dir.as_deref(),
        cli.download_symbols,
        show_all_threads,
        show_registers,
    )
    .await?;

    if cli.download_symbols {
        return Ok(());
    }

    let output = if cli.json {
        serde_json::to_string_pretty(&report)?
    } else {
        minidump_analyzer::analyzer::format_text(&report, show_all_threads, show_registers)
    };

    if let Some(ref path) = cli.output {
        std::fs::write(path, &output)?;
        eprintln!("报告已保存: {}", path.display());
    } else {
        print!("{output}");
    }

    Ok(())
}
