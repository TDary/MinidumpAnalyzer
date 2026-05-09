use anyhow::{Context, Result};
use minidump::{MinidumpModuleList, Module};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use crate::Verbosity;

const MICROSOFT_SYMBOL_SERVER: &str = "https://msdl.microsoft.com/download/symbols";

pub fn guid_to_breakpad_id(guid: &str) -> String {
    guid.replace('-', "").to_uppercase()
}

pub fn check_dump_syms() -> Result<()> {
    Command::new("dump_syms")
        .arg("--help")
        .output()
        .map(|_| ())
        .context("未找到 dump_syms，请安装: cargo install dump_syms")
}

pub fn sym_exists(cache_dir: &Path, pdb_name: &str, breakpad_id: &str) -> bool {
    let sym_name = pdb_name.replace(".pdb", ".sym");
    cache_dir
        .join(pdb_name)
        .join(breakpad_id)
        .join(&sym_name)
        .exists()
}

fn dump_syms_convert(
    target_dir: &Path,
    pdb_path: &Path,
    pdb_name: &str,
    verbosity: Verbosity,
) -> Result<()> {
    let tmp_pdb = target_dir.join(pdb_name);
    std::fs::copy(pdb_path, &tmp_pdb)?;

    if !verbosity.is_silent() {
        eprintln!("  [CONVERT] {} ...", pdb_name);
    }
    let started = Instant::now();
    let output = Command::new("dump_syms")
        .arg(&tmp_pdb)
        .output()
        .with_context(|| "执行 dump_syms 失败，请确认已安装: cargo install dump_syms")?;

    let _ = std::fs::remove_file(&tmp_pdb);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("dump_syms 执行失败: {}", stderr);
    }

    if verbosity == Verbosity::Verbose {
        eprintln!(
            "          转换耗时: {:.1}s",
            started.elapsed().as_secs_f32()
        );
    }

    let sym_name = pdb_name.replace(".pdb", ".sym");
    std::fs::write(target_dir.join(&sym_name), &output.stdout)?;
    Ok(())
}

async fn download_and_convert(
    cache_dir: &Path,
    pdb_name: &str,
    breakpad_id: &str,
    verbosity: Verbosity,
) -> Result<()> {
    let sym_name = pdb_name.replace(".pdb", ".sym");
    let target_dir = cache_dir.join(pdb_name).join(breakpad_id);
    let target_file = target_dir.join(&sym_name);

    if target_file.exists() {
        if !verbosity.is_silent() {
            eprintln!("  [SKIP] {} (已存在)", pdb_name);
        }
        return Ok(());
    }

    std::fs::create_dir_all(&target_dir)?;

    let url = format!(
        "{}/{}/{}/{}",
        MICROSOFT_SYMBOL_SERVER, pdb_name, breakpad_id, pdb_name
    );

    if !verbosity.is_silent() {
        eprintln!("  [DOWNLOAD] {} ...", pdb_name);
    }
    let started = Instant::now();
    let resp = reqwest::get(&url)
        .await
        .with_context(|| format!("下载失败: {}", url))?;

    if !resp.status().is_success() {
        if !verbosity.is_silent() {
            eprintln!(
                "  [SKIP] {} (HTTP {}, 符号可能不存在)",
                pdb_name,
                resp.status()
            );
        }
        return Ok(());
    }

    let pdb_data = resp.bytes().await?;
    if pdb_data.is_empty() {
        anyhow::bail!("下载的文件为空");
    }

    if verbosity == Verbosity::Verbose {
        eprintln!(
            "          下载耗时: {:.1}s, 大小: {} KB",
            started.elapsed().as_secs_f32(),
            pdb_data.len() / 1024
        );
    }

    let tmp_pdb = target_dir.join(pdb_name);
    std::fs::write(&tmp_pdb, &pdb_data)?;

    if !verbosity.is_silent() {
        eprintln!("  [CONVERT] {} ...", pdb_name);
    }
    let conv_started = Instant::now();
    let output = Command::new("dump_syms")
        .arg(&tmp_pdb)
        .output()
        .with_context(|| "执行 dump_syms 失败，请确认已安装: cargo install dump_syms")?;

    let _ = std::fs::remove_file(&tmp_pdb);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("dump_syms 执行失败: {}", stderr);
    }

    if verbosity == Verbosity::Verbose {
        eprintln!(
            "          转换耗时: {:.1}s",
            conv_started.elapsed().as_secs_f32()
        );
    }

    std::fs::write(&target_file, &output.stdout)?;
    if !verbosity.is_silent() {
        eprintln!("  [OK] {}", target_file.display());
    }
    Ok(())
}

pub async fn download_missing_symbols(
    modules: &MinidumpModuleList,
    symbols_dir: &Path,
    cache_dir: &Path,
    pdb_dir: Option<&Path>,
    include_remote: bool,
    verbosity: Verbosity,
) -> Result<()> {
    let symbols_dir = symbols_dir.to_path_buf();
    let cache_dir = cache_dir.to_path_buf();
    let pdb_dir = pdb_dir.map(|p| p.to_path_buf());

    let mut tasks = tokio::task::JoinSet::new();
    let mut total = 0u32;
    let mut skipped = 0u32;

    let overall = Instant::now();

    for m in modules.iter() {
        let Some(debug_file) = m.debug_file() else {
            continue;
        };
        let Some(debug_id) = m.debug_identifier() else {
            continue;
        };

        let pdb_name = Path::new(debug_file.as_ref())
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| debug_file.to_string());

        let breakpad_id = guid_to_breakpad_id(&debug_id.to_string());
        total += 1;

        if sym_exists(&symbols_dir, &pdb_name, &breakpad_id)
            || sym_exists(&cache_dir, &pdb_name, &breakpad_id)
        {
            skipped += 1;
            if verbosity == Verbosity::Verbose {
                eprintln!("  [CACHED] {} / {}", pdb_name, breakpad_id);
            }
            continue;
        }

        if verbosity == Verbosity::Verbose {
            eprintln!("  [CHECK] {} / {}", pdb_name, breakpad_id);
        }

        // Check local PDB directory first
        if let Some(ref pdb) = pdb_dir {
            let local_pdb = pdb.join(&pdb_name);
            if local_pdb.exists() {
                let cache = cache_dir.clone();
                let name = pdb_name;
                let bid = breakpad_id;
                let target_dir = cache.join(&name).join(&bid);
                tasks.spawn(async move {
                    let _ = std::fs::create_dir_all(&target_dir);
                    let result = dump_syms_convert(&target_dir, &local_pdb, &name, verbosity);
                    match &result {
                        Ok(()) => {
                            if !verbosity.is_silent() {
                                eprintln!(
                                    "  [OK] {}/{}/{}.sym",
                                    name,
                                    bid,
                                    name.replace(".pdb", "")
                                )
                            }
                        }
                        Err(e) => {
                            if !verbosity.is_silent() {
                                eprintln!("  [FAIL] {}: {}", name, e)
                            }
                        }
                    }
                    (name, result)
                });
                continue;
            }
        }

        // Download from Microsoft
        if include_remote {
            let cache = cache_dir.clone();
            let name = pdb_name;
            let bid = breakpad_id;
            tasks.spawn(async move {
                let result = download_and_convert(&cache, &name, &bid, verbosity).await;
                (name, result)
            });
        }
    }

    let mut ok = 0u32;
    let mut fail = 0u32;

    while let Some(result) = tasks.join_next().await {
        if let Ok((pdb_name, result)) = result {
            match result {
                Ok(()) => ok += 1,
                Err(e) => {
                    if !verbosity.is_silent() {
                        eprintln!("  [FAIL] {}: {}", pdb_name, e);
                    }
                    fail += 1;
                }
            }
        }
    }

    if !verbosity.is_silent() {
        eprintln!(
            "\n符号获取完成: 总计={}, 成功={}, 跳过(已存在)={}, 失败={}",
            total, ok, skipped, fail
        );
        if verbosity == Verbosity::Verbose {
            eprintln!("总耗时: {:.1}s", overall.elapsed().as_secs_f32());
        }
    }
    Ok(())
}
