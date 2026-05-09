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
- `--registers` 输出崩溃线程寄存器上下文
- `--all-threads` 输出所有线程的调用栈
- `--json` 输出结构化 JSON

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
