# MinidumpAnalyzer

基于 [rust-minidump](https://github.com/rust-minidump/rust-minidump) 的 Windows Minidump 崩溃堆栈解析工具。

## 功能

- 解析 `.dmp` 文件，输出系统信息、异常信息、加载模块列表
- 解析崩溃线程调用栈，输出函数名、源文件及行号
- 支持 `--registers` 输出崩溃线程寄存器上下文
- 支持 `--all-threads` 输出所有线程的调用栈
- 支持 `--json` 以结构化 JSON 格式输出
- 从微软符号服务器并发下载 PDB 符号文件并转换为 Breakpad `.sym` 格式
- 支持本地符号缓存，避免重复下载

## 依赖

- [Rust](https://www.rust-lang.org/) (stable)
- [dump_syms](https://github.com/rust-minidump/rust-minidump) — PDB 转 Breakpad 符号工具

```bash
cargo install dump_syms
```

## 构建

```bash
cargo build --release
```

## 用法

```bash
# 基本解析
minidump-analyzer crash.dmp

# 指定符号目录和缓存目录
minidump-analyzer -s ./symbols -c ./sym_cache crash.dmp

# 仅下载缺失的符号
minidump-analyzer --download-symbols crash.dmp

# 显示所有线程的调用栈
minidump-analyzer --all-threads crash.dmp

# 显示寄存器上下文
minidump-analyzer --registers crash.dmp

# 完整输出（= --all-threads --registers）
minidump-analyzer --full crash.dmp

# JSON 格式输出
minidump-analyzer --json crash.dmp
```

### 选项

| 选项 | 说明 |
| ---- | ---- |
| `-s, --symbols-dir <路径>` | 本地 `.sym` 符号目录 (默认: `./symbols`) |
| `-c, --cache-dir <路径>` | 符号缓存目录 (默认: `./sym_cache`) |
| `--download-symbols` | 仅下载缺失符号，不解析 dmp |
| `--all-threads` | 输出所有线程的调用栈 |
| `--registers` | 输出崩溃线程的寄存器上下文 |
| `--full` | 等价于 `--all-threads --registers` |
| `--json` | 以 JSON 格式输出分析结果 |
| `-h, --help` | 显示帮助 |

### 符号目录结构

```text
<symbols_dir>/
  ntdll.pdb/
    <BREAKPAD_ID>/
      ntdll.sym
```

## 辅助脚本

[dowload_symbols.sh](dowload_symbols.sh) — 独立的 bash 脚本，可从微软符号服务器批量下载 PDB 并转换，适合在非 Rust 环境使用。
