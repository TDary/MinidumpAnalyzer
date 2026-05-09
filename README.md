# MinidumpAnalyzer

基于 [rust-minidump](https://github.com/rust-minidump/rust-minidump) 的 Windows Minidump 崩溃堆栈解析工具。

## 快速开始

```bash
# 1. 安装依赖
cargo install dump_syms

# 2. 构建
cargo build --release

# 3. 跑分析（一条命令：自动转换 PDB、下载系统符号、解析堆栈）
./target/release/minidump-analyzer -p ./my_pdbs crash.dmp
```

首次运行会自动下载系统符号并将项目 PDB 转换为 `.sym`，全部缓存到 `./sym_cache`。后续运行直接读缓存，秒出结果。

## 功能

- 解析 `.dmp` 文件，输出系统信息、异常信息、加载模块列表
- 解析崩溃线程调用栈，输出函数名、源文件及行号
- `-p` 指定项目 PDB 目录（扁平目录即可），自动转为 Breakpad 符号
- 从微软符号服务器并发下载系统符号并缓存
- `--registers` 输出崩溃线程寄存器上下文（x86/x64/ARM64）
- `--all-threads` 输出所有线程的调用栈
- `--json` 输出结构化 JSON
- `-q` 静默模式，`-v` 详细模式（显示逐模块检查状态和耗时）

## MCP Server

可以将分析能力暴露为 MCP 工具，让 Claude Code 直接调用：

```bash
# 1. 构建
cargo build --release -p minidump-analyzer-mcp

# 2. 注册到 Claude Code
claude mcp add --transport stdio minidump-analyzer -- \
  e:/MinidumpAnalyzer/target/release/minidump-analyzer-mcp.exe
```

注册后 Claude 可直接调用两个工具：

| 工具 | 功能 |
| ---- | ---- |
| `analyze_dump` | 解析 .dmp，返回 JSON 报告（系统信息、异常、模块、线程调用栈、寄存器） |
| `download_symbols` | 预取符号：本地 PDB 自动转换 + Microsoft Symbol Server 下载 |

Skill 文件（[.claude/skills/minidump-analyzer.md](.claude/skills/minidump-analyzer.md)）提供崩溃诊断领域知识，Claude 会自动加载。

## 架构

```
┌──────────────────────────────────────────────────────┐
│                    入口层                            │
│  ┌──────────────────┐  ┌──────────────────────────┐  │
│  │  CLI (main.rs)   │  │  MCP Server (mcp/)       │  │
│  │  clap 参数解析    │  │  rmcp stdio transport    │  │
│  │  -p/-q/-v/--json │  │  analyze_dump            │  │
│  │  -o/--full/...   │  │  download_symbols        │  │
│  └────────┬─────────┘  └───────────┬──────────────┘  │
│           │                        │                  │
├───────────┼────────────────────────┼──────────────────┤
│           ▼                        ▼                  │
│  ┌───────────────────────────────────────────────┐   │
│  │              lib.rs · analyze()               │   │
│  │         读取 dmp → 符号预取 → 解析堆栈          │   │
│  └──────┬──────────────────────────┬─────────────┘   │
│         │                          │                  │
├─────────┼──────────────────────────┼──────────────────┤
│         ▼                          ▼                  │
│  ┌──────────────┐   ┌──────────────────────────┐     │
│  │  symbols.rs  │   │  analyzer.rs              │     │
│  │              │   │                           │     │
│  │ PDB→.sym     │   │ CrashReport / 结构体      │     │
│  │ MS 符号下载   │   │ build_report()            │     │
│  │ 缓存管理      │   │ format_text()             │     │
│  │ dump_syms    │   │ extract_registers()       │     │
│  └──────┬───────┘   │   x86 / x64 / ARM64       │     │
│         │           └──────────┬────────────────┘     │
│         │                      │                      │
├─────────┼──────────────────────┼──────────────────────┤
│         ▼                      ▼                      │
│  ┌──────────────────────────────────────────────┐    │
│  │            外部依赖                           │    │
│  │  rust-minidump · dump_syms · reqwest          │    │
│  │  Microsoft Symbol Server                      │    │
│  └──────────────────────────────────────────────┘    │
│                                                       │
│  .claude/skills/minidump-analyzer.md                  │
│  └─ Claude Code 自动加载，提供崩溃诊断领域知识        │
└──────────────────────────────────────────────────────┘
```

**数据流:** `.dmp 文件 → Minidump::read_path() → 提取流 (系统信息/异常/模块) → 符号预取 (PDB转换/MS下载) → http_symbol_supplier 解析堆栈 → build_report() → 文本/JSON 输出`

## 依赖

- [Rust](https://www.rust-lang.org/) (stable)
- [dump_syms](https://github.com/rust-minidump/rust-minidump)

## 用法

```bash
# 基本解析（仅系统符号，从微软下载）
minidump-analyzer crash.dmp

# 带项目 PDB，自动转换
minidump-analyzer -p ./my_pdbs crash.dmp

# 自定义目录
minidump-analyzer -s ./symbols -c ./sym_cache -p ./my_pdbs crash.dmp

# 只下载/转换符号，不解析
minidump-analyzer --download-symbols -p ./my_pdbs crash.dmp

# 扩展输出
minidump-analyzer --all-threads crash.dmp        # 所有线程调用栈
minidump-analyzer --registers crash.dmp          # 崩溃线程寄存器
minidump-analyzer --full crash.dmp               # = --all-threads --registers
minidump-analyzer --json --full crash.dmp        # JSON 格式完整输出

# 生成报告文件
minidump-analyzer --full -o report.txt crash.dmp
minidump-analyzer --json --full -o report.json crash.dmp

# 脚本化（静默 + 文件输出）
minidump-analyzer -q --json --full -o report.json crash.dmp

# 调试符号问题（显示逐模块检查状态和耗时）
minidump-analyzer -v -p ./my_pdbs crash.dmp
```

### 选项

| 选项 | 说明 |
| ---- | ---- |
| `-s, --symbols-dir` | 本地 Breakpad `.sym` 符号目录 (默认 `./symbols`) |
| `-c, --cache-dir` | 符号缓存目录，PDB 转换和下载的符号都存这里 (默认 `./sym_cache`) |
| `-p, --pdb-dir` | 项目 PDB 目录，工具自动按文件名匹配并用 dump_syms 转换 |
| `--download-symbols` | 仅下载/转换缺失符号，不解析 dmp |
| `--all-threads` | 输出所有线程的调用栈 |
| `--registers` | 输出崩溃线程的寄存器上下文 |
| `--full` | 等价于 `--all-threads --registers` |
| `--json` | 以 JSON 格式输出分析结果 |
| `-o, --output <路径>` | 将报告写入文件，不指定则输出到 stdout |
| `-q, --quiet` | 静默模式，不输出进度信息到 stderr（适合脚本） |
| `-v, --verbose` | 详细模式，输出逐模块检查状态和操作耗时（与 `-q` 互斥） |
| `-h, --help` | 显示帮助 |

### 符号目录说明

**`-p` (PDB 目录)** — 放原始 `.pdb` 文件即可，扁平目录，不需要任何目录结构：

```text
my_pdbs/
  MyApp.pdb
  SomeLib.pdb
```

**`-s` (符号目录)** — 放已经转换好的 `.sym` 文件，需要按 Breakpad 约定组织：

```text
symbols/
  MyApp.pdb/
    <BREAKPAD_ID>/
      MyApp.sym
```

**`-c` (缓存目录)** — 工具自动管理，无需手动操作。
