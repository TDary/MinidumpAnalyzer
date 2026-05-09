---
name: minidump-analyzer
description: Analyze Windows minidump (.dmp) crash files with full symbol resolution. Use when the user needs to diagnose crashes, analyze dmp files, or investigate Windows exceptions.
---

## Tools

This skill provides access to the `minidump-analyzer` MCP server with two tools:

### analyze_dump
Analyzes a `.dmp` file and returns a structured JSON report containing:
- System info (OS, CPU architecture)
- Exception details (code, address, reason)
- Loaded module list with symbol resolution status
- Thread callstacks with resolved function names, source files, and line numbers
- Register context (when requested)

### download_symbols
Pre-fetches missing PDB symbols from local PDB files or Microsoft Symbol Server.

## Workflow

1. **Assess what's available**: Ask the user for the `.dmp` file path, and optionally the PDB directory or symbol directory.
2. **Pre-fetch symbols** (optional but recommended): Run `download_symbols` with the dmp file path. If the user has local PDB files, include `pdb_dir`.
3. **Analyze the crash**: Run `analyze_dump` with `all_threads: true` and `registers: true` for a comprehensive report.
4. **Read and diagnose**: Read the complete JSON report and look for:
   - **Exception code** — the crash category (see table below)
   - **Exception address** — the instruction pointer at crash time
   - **Crash thread top frames** — the call chain leading to the crash
   - **Register values** — especially RIP/RSP/RBP for x64, EIP/ESP/EBP for x86

## Exception Code Reference

| Code | Name | Typical Cause |
|------|------|---------------|
| `0xC0000005` | ACCESS_VIOLATION | Null pointer, use-after-free, buffer overflow |
| `0xC00000FD` | STACK_OVERFLOW | Infinite recursion, large stack allocation |
| `0xC0000094` | INT_DIVIDE_BY_ZERO | Integer division by zero |
| `0xC0000096` | PRIV_INSTRUCTION | Privileged instruction executed in user mode |
| `0xC000001D` | ILLEGAL_INSTRUCTION | Invalid or corrupted instruction |
| `0xC0000006` | IN_PAGE_ERROR | Disk I/O error during page-in |
| `0xC0000017` | NO_MEMORY | Out of memory |
| `0x80000003` | BREAKPOINT | Intentional breakpoint or assert failure |
| `0x80000004` | SINGLE_STEP | Single-step execution |

## Diagnosis Tips

- **ACCESS_VIOLATION at small address** (e.g., `0x0000000000000000` or near zero): null pointer dereference
- **ACCESS_VIOLATION at freed pattern** (`0xFEEEFEEE`, `0xDDDDDDDD`, `0xCDCDCDCD`): use-after-free
- **Top frame is system DLL** (ntdll, kernel32): the crash may have corrupted the stack — look at the modules list for suspicious ones
- **Top frame is unknown** (`<unknown>` or no symbols): missing symbols — run `download_symbols` first
- **STACK_OVERFLOW with repeated function names**: infinite recursion — count the identical frames
- **Exception address in a known module**: compare with the module list to identify which component crashed
