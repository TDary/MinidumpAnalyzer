use minidump_analyzer::{Verbosity, analyze};
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo, ToolsCapability};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;

// ── Server state ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MinidumpServer {
    tool_router: ToolRouter<Self>,
}

impl MinidumpServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

// ── Tool input types ────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AnalyzeDumpInput {
    /// Path to the .dmp file to analyze
    pub dmp_path: String,
    /// Local Breakpad .sym symbols directory (default: ./symbols)
    #[serde(default = "default_symbols_dir")]
    pub symbols_dir: String,
    /// Symbol cache directory (default: ./sym_cache)
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    /// Local PDB directory for auto-conversion via dump_syms
    pub pdb_dir: Option<String>,
    /// Include all threads' callstacks (default: false)
    #[serde(default)]
    pub all_threads: bool,
    /// Include crash thread register context (default: false)
    #[serde(default)]
    pub registers: bool,
}

fn default_symbols_dir() -> String {
    "./symbols".to_string()
}
fn default_cache_dir() -> String {
    "./sym_cache".to_string()
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DownloadSymbolsInput {
    /// Path to the .dmp file
    pub dmp_path: String,
    /// Local Breakpad .sym symbols directory (default: ./symbols)
    #[serde(default = "default_symbols_dir")]
    pub symbols_dir: String,
    /// Symbol cache directory (default: ./sym_cache)
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    /// Local PDB directory for auto-conversion
    pub pdb_dir: Option<String>,
}

// ── Tools ───────────────────────────────────────────────────────────

#[tool_router]
impl MinidumpServer {
    #[tool(
        description = "Analyze a Windows minidump (.dmp) crash file. Returns structured JSON with: system info (OS, CPU), exception details (code, address, reason), loaded modules with symbol status, and thread callstacks with resolved function names, source files, and line numbers."
    )]
    async fn analyze_dump(&self, params: Parameters<AnalyzeDumpInput>) -> String {
        let symbols_path = PathBuf::from(&params.0.symbols_dir);
        let cache_path = PathBuf::from(&params.0.cache_dir);
        let pdb_path = params.0.pdb_dir.as_ref().map(PathBuf::from);

        match analyze(
            &params.0.dmp_path,
            &symbols_path,
            &cache_path,
            pdb_path.as_deref(),
            false,
            params.0.all_threads,
            params.0.registers,
            Verbosity::Quiet,
        )
        .await
        {
            Ok(report) => match serde_json::to_string_pretty(&report) {
                Ok(json) => json,
                Err(e) => format!("JSON serialization failed: {e}"),
            },
            Err(e) => format!("Analysis failed: {e:#}"),
        }
    }

    #[tool(
        description = "Download or convert missing PDB symbols for a minidump file. Resolves symbols from: 1) local PDB files (auto-converted via dump_syms), 2) Microsoft Symbol Server. Caches results for future use. Run this first if symbols are missing."
    )]
    async fn download_symbols(&self, params: Parameters<DownloadSymbolsInput>) -> String {
        let symbols_path = PathBuf::from(&params.0.symbols_dir);
        let cache_path = PathBuf::from(&params.0.cache_dir);
        let pdb_path = params.0.pdb_dir.as_ref().map(PathBuf::from);

        if pdb_path.is_some() {
            if let Err(e) = minidump_analyzer::symbols::check_dump_syms() {
                return format!(
                    "dump_syms not available: {e:#}. Install with: cargo install dump_syms"
                );
            }
        }

        match analyze(
            &params.0.dmp_path,
            &symbols_path,
            &cache_path,
            pdb_path.as_deref(),
            true,
            false,
            false,
            Verbosity::Quiet,
        )
        .await
        {
            Ok(_) => "Symbol download/conversion completed successfully. Run analyze_dump to analyze the crash now.".to_string(),
            Err(e) => format!("Symbol download failed: {e:#}"),
        }
    }
}

// ── Server handler ──────────────────────────────────────────────────

#[tool_handler]
impl ServerHandler for MinidumpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "minidump-analyzer".into(),
                title: Some("Windows Minidump Crash Analyzer".into()),
                version: env!("CARGO_PKG_VERSION").into(),
                description: Some("Analyzes .dmp files with symbol resolution from Microsoft Symbol Server and local PDBs.".into()),
                ..Default::default()
            },
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            },
            instructions: Some(
                concat!(
                    "Analyzes Windows minidump (.dmp) crash files with full symbol resolution.\n\n",
                    "Workflow:\n",
                    "1. If you have local PDB files, run download_symbols first to convert them.\n",
                    "2. Run analyze_dump with all_threads and registers for comprehensive analysis.\n",
                    "3. Interpret the crash - key things to check:\n",
                    "   - Exception code: ACCESS_VIOLATION (0xC0000005) = null pointer/use-after-free/buffer overflow\n",
                    "   - Exception address: where the crash occurred (the instruction pointer)\n",
                    "   - Crash thread's top frames: the exact call chain leading to the crash\n",
                    "   - Register values: RIP (instruction pointer), RSP (stack pointer), RBP (base pointer) for x64\n",
                    "\nCommon crash codes:\n",
                    "  0xC0000005 ACCESS_VIOLATION - null pointer deref, use-after-free, buffer overflow\n",
                    "  0xC00000FD STACK_OVERFLOW - infinite recursion or large stack allocation\n",
                    "  0xC0000094 INT_DIVIDE_BY_ZERO - integer division by zero\n",
                    "  0x80000003 BREAKPOINT - intentional breakpoint or failed assertion\n",
                    "  0xC0000017 NO_MEMORY - out of memory\n",
                ).into(),
            ),
            ..Default::default()
        }
    }
}

// ── Main ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let server = MinidumpServer::new();
    let transport = rmcp::transport::io::stdio();
    let server_handle = server.serve(transport).await?;
    server_handle.waiting().await?;
    Ok(())
}
