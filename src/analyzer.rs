use minidump::{
    MinidumpContext, MinidumpException, MinidumpModuleList, MinidumpRawContext, MinidumpSystemInfo,
    Module,
};
use minidump_processor::ProcessState;
use serde::Serialize;
use std::path::Path;

use crate::Channel;
use crate::symbols;

// ── Output data structures ────────────────────────────────────────────

#[derive(Serialize)]
pub struct CrashReport {
    pub channel: Channel,
    pub system_info: Option<SystemInfoOutput>,
    pub exception: Option<ExceptionOutput>,
    pub modules: Vec<ModuleOutput>,
    pub threads: Vec<ThreadOutput>,
}

#[derive(Serialize)]
pub struct SystemInfoOutput {
    pub os: String,
    pub cpu: String,
}

#[derive(Serialize)]
pub struct ExceptionOutput {
    pub code: String,
    pub address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct ModuleOutput {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_id: Option<String>,
    pub has_symbol: bool,
}

#[derive(Serialize)]
pub struct ThreadOutput {
    pub index: usize,
    pub is_crash_thread: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registers: Option<Vec<RegisterOutput>>,
    pub frames: Vec<FrameOutput>,
}

#[derive(Serialize)]
pub struct RegisterOutput {
    pub register: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct FrameOutput {
    pub index: usize,
    pub function: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

// ── Exception reason mapping ──────────────────────────────────────────

pub(crate) fn exception_reason(code: u32) -> &'static str {
    match code {
        0xC0000005 => "ACCESS_VIOLATION",
        0xC0000094 => "INT_DIVIDE_BY_ZERO",
        0xC00000FD => "STACK_OVERFLOW",
        0xC0000096 => "PRIV_INSTRUCTION",
        0xC000001D => "ILLEGAL_INSTRUCTION",
        0xC0000006 => "IN_PAGE_ERROR",
        0xC0000017 => "NO_MEMORY",
        0x80000003 => "BREAKPOINT",
        0x80000004 => "SINGLE_STEP",
        _ => "UNKNOWN",
    }
}

// ── Register extraction ───────────────────────────────────────────────

fn extract_arm64_regs(
    iregs: &[u64; 31],
    sp: u64,
    pc: u64,
    cpsr: impl Into<u64>,
) -> Vec<RegisterOutput> {
    const NAMES: [&str; 31] = [
        "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13",
        "x14", "x15", "x16", "x17", "x18", "x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26",
        "x27", "x28", "fp", "lr",
    ];
    let mut regs: Vec<(&str, u64)> = Vec::with_capacity(35);
    for (i, &val) in iregs.iter().enumerate() {
        regs.push((NAMES[i], val));
    }
    regs.push(("sp", sp));
    regs.push(("pc", pc));
    regs.push(("cpsr", cpsr.into()));
    regs.into_iter()
        .map(|(name, val)| RegisterOutput {
            register: name.to_string(),
            value: format!("0x{:016X}", val),
        })
        .collect()
}

fn extract_registers(context: &MinidumpContext) -> Vec<RegisterOutput> {
    use MinidumpRawContext::*;

    match &context.raw {
        Amd64(ctx) => {
            let regs: &[(&str, u64)] = &[
                ("rip", ctx.rip),
                ("rsp", ctx.rsp),
                ("rbp", ctx.rbp),
                ("rax", ctx.rax),
                ("rbx", ctx.rbx),
                ("rcx", ctx.rcx),
                ("rdx", ctx.rdx),
                ("rsi", ctx.rsi),
                ("rdi", ctx.rdi),
                ("r8", ctx.r8),
                ("r9", ctx.r9),
                ("r10", ctx.r10),
                ("r11", ctx.r11),
                ("r12", ctx.r12),
                ("r13", ctx.r13),
                ("r14", ctx.r14),
                ("r15", ctx.r15),
                ("eflags", ctx.eflags as u64),
            ];
            regs.iter()
                .map(|(name, val)| RegisterOutput {
                    register: name.to_string(),
                    value: format!("0x{:016X}", val),
                })
                .collect()
        }
        X86(ctx) => {
            let regs: &[(&str, u32)] = &[
                ("eip", ctx.eip),
                ("esp", ctx.esp),
                ("ebp", ctx.ebp),
                ("eax", ctx.eax),
                ("ebx", ctx.ebx),
                ("ecx", ctx.ecx),
                ("edx", ctx.edx),
                ("esi", ctx.esi),
                ("edi", ctx.edi),
                ("eflags", ctx.eflags),
            ];
            regs.iter()
                .map(|(name, val)| RegisterOutput {
                    register: name.to_string(),
                    value: format!("0x{:08X}", val),
                })
                .collect()
        }
        Arm64(ctx) => extract_arm64_regs(&ctx.iregs, ctx.sp, ctx.pc, ctx.cpsr),
        OldArm64(ctx) => extract_arm64_regs(&ctx.iregs, ctx.sp, ctx.pc, ctx.cpsr),
        _ => vec![],
    }
}

// ── Report builder ────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn build_report(
    sys_info: Option<MinidumpSystemInfo>,
    exception: Option<MinidumpException>,
    modules: Option<MinidumpModuleList>,
    context: Option<MinidumpContext>,
    state: &ProcessState,
    symbols_dir: &Path,
    cache_dir: &Path,
    include_all_threads: bool,
    include_registers: bool,
    channel: Channel,
) -> CrashReport {
    let system_info = sys_info.map(|s| SystemInfoOutput {
        os: format!("{:?}", s.os),
        cpu: format!("{:?}", s.cpu),
    });

    let exception = exception.map(|exc| {
        let record = &exc.raw.exception_record;
        let code = record.exception_code;
        ExceptionOutput {
            code: format!("0x{:08X}", code),
            address: format!("0x{:016X}", record.exception_address),
            reason: Some(exception_reason(code).to_string()),
        }
    });

    let modules = modules
        .map(|ml| {
            ml.iter()
                .map(|m| {
                    let name = Path::new(m.code_file().as_ref())
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| m.code_file().to_string());
                    let debug_file = m.debug_file().map(|d| d.to_string());
                    let (debug_file_name, debug_id_str, has_symbol) = m
                        .debug_identifier()
                        .map(|id| {
                            let df = debug_file
                                .as_deref()
                                .and_then(|df| {
                                    Path::new(df)
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                })
                                .unwrap_or_else(|| debug_file.clone().unwrap_or_default());
                            let bid = symbols::guid_to_breakpad_id(&id.to_string());
                            let has = symbols::sym_exists(symbols_dir, &df, &bid)
                                || symbols::sym_exists(cache_dir, &df, &bid);
                            (Some(df), Some(bid), has)
                        })
                        .unwrap_or((debug_file, None, false));

                    ModuleOutput {
                        name,
                        debug_file: debug_file_name,
                        debug_id: debug_id_str,
                        has_symbol,
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let crash_thread_idx = state.requesting_thread;
    let threads = state
        .threads
        .iter()
        .enumerate()
        .map(|(i, thread)| {
            let is_crash = Some(i) == crash_thread_idx;
            let frames: Vec<FrameOutput> = if is_crash || include_all_threads {
                thread
                    .frames
                    .iter()
                    .filter(|f| {
                        f.function_name
                            .as_deref()
                            .is_some_and(|n| !n.starts_with("<unknown") && !n.starts_with("<未知"))
                    })
                    .enumerate()
                    .map(|(j, f)| FrameOutput {
                        index: j,
                        function: f.function_name.as_deref().unwrap().to_string(),
                        file: f.source_file_name.clone(),
                        line: f.source_line,
                    })
                    .collect()
            } else {
                vec![]
            };

            let registers = if is_crash && include_registers {
                context.as_ref().map(extract_registers)
            } else {
                None
            };

            ThreadOutput {
                index: i,
                is_crash_thread: is_crash,
                registers,
                frames,
            }
        })
        .collect();

    CrashReport {
        channel,
        system_info,
        exception,
        modules,
        threads,
    }
}

// ── Text output ───────────────────────────────────────────────────────

pub fn format_text(
    report: &CrashReport,
    include_all_threads: bool,
    include_registers: bool,
) -> String {
    use std::fmt::Write;

    let mut out = String::new();

    // Channel
    let _ = writeln!(out, "渠道: {:?}", report.channel);

    // System info
    if let Some(ref si) = report.system_info {
        let _ = writeln!(out, "--- 系统信息 ---");
        let _ = writeln!(out, "操作系统: {}", si.os);
        let _ = writeln!(out, "CPU 架构: {}", si.cpu);
    }

    // Exception
    if let Some(ref exc) = report.exception {
        let _ = writeln!(out, "\n--- 异常信息 ---");
        let _ = writeln!(out, "异常码:  {}", exc.code);
        let _ = writeln!(out, "异常地址: {}", exc.address);
        if let Some(ref reason) = exc.reason {
            let _ = writeln!(out, "异常原因: {}", reason);
        }
    }

    // Modules
    let _ = writeln!(out, "\n--- 模块列表 ---");
    for m in &report.modules {
        let tag = if m.has_symbol { " [sym]" } else { "" };
        match (&m.debug_file, &m.debug_id) {
            (Some(df), Some(id)) => {
                let _ = writeln!(out, "  {:<40} {:<30} {}{}", m.name, df, id, tag);
            }
            _ => {
                let _ = writeln!(out, "  {:<40} <无调试信息>", m.name);
            }
        }
    }

    // Threads
    for thread in &report.threads {
        if !thread.is_crash_thread && !include_all_threads {
            continue;
        }

        if thread.is_crash_thread {
            let _ = writeln!(out, "\n--- 崩溃调用栈 (线程 #{}) ---", thread.index);
        } else {
            let _ = writeln!(out, "\n--- 线程 #{} ---", thread.index);
        }

        if include_registers && let Some(ref regs) = thread.registers {
            let _ = writeln!(out, "  寄存器:");
            for chunk in regs.chunks(4) {
                let line: Vec<String> = chunk
                    .iter()
                    .map(|r| format!("{:<6} {}", r.register, r.value))
                    .collect();
                let _ = writeln!(out, "    {}", line.join("  "));
            }
        }

        if thread.frames.is_empty() {
            let _ = writeln!(out, "  <无已解析的调用帧>");
            continue;
        }

        for frame in &thread.frames {
            let location = match (&frame.file, frame.line) {
                (Some(f), Some(l)) if l > 0 => format!("{}:{}", f, l),
                (Some(f), _) => f.clone(),
                _ => "<未知位置>".to_string(),
            };
            let _ = writeln!(
                out,
                "#{:>2} {:<50} ({})",
                frame.index, frame.function, location
            );
        }
    }

    if !include_all_threads {
        let other_count = report.threads.iter().filter(|t| !t.is_crash_thread).count();
        if other_count > 0 {
            let _ = writeln!(
                out,
                "\n其他 {} 个线程的调用栈 (使用 --all-threads 查看)",
                other_count
            );
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_arm64_regs() {
        let mut iregs = [0u64; 31];
        iregs[0] = 0xDEADBEEF; // x0
        iregs[29] = 0xCAFE; // fp (x29)
        iregs[30] = 0xBABE; // lr (x30)

        let regs = extract_arm64_regs(&iregs, 0x1000, 0x2000, 0x60000000u32);

        assert_eq!(regs.len(), 34); // 31 iregs + sp + pc + cpsr
        assert_eq!(regs[0].register, "x0");
        assert_eq!(regs[0].value, "0x00000000DEADBEEF");
        assert_eq!(regs[29].register, "fp");
        assert_eq!(regs[29].value, "0x000000000000CAFE");
        assert_eq!(regs[30].register, "lr");
        assert_eq!(regs[30].value, "0x000000000000BABE");
        assert_eq!(regs[31].register, "sp");
        assert_eq!(regs[31].value, "0x0000000000001000");
        assert_eq!(regs[32].register, "pc");
        assert_eq!(regs[32].value, "0x0000000000002000");
        assert_eq!(regs[33].register, "cpsr");
        assert_eq!(regs[33].value, "0x0000000060000000");
    }
}
