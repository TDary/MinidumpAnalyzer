use anyhow::{Context, Result};
use minidump::{MinidumpModuleList, Module};
use std::path::Path;
use std::process::Command;

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

fn dump_syms_convert(target_dir: &Path, pdb_path: &Path, pdb_name: &str) -> Result<()> {
    let tmp_pdb = target_dir.join(pdb_name);
    std::fs::copy(pdb_path, &tmp_pdb)?;

    eprintln!("  [CONVERT] {} ...", pdb_name);
    let output = Command::new("dump_syms")
        .arg(&tmp_pdb)
        .output()
        .with_context(|| "执行 dump_syms 失败，请确认已安装: cargo install dump_syms")?;

    let _ = std::fs::remove_file(&tmp_pdb);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("dump_syms 执行失败: {}", stderr);
    }

    let sym_name = pdb_name.replace(".pdb", ".sym");
    std::fs::write(target_dir.join(&sym_name), &output.stdout)?;
    Ok(())
}

async fn download_and_convert(cache_dir: &Path, pdb_name: &str, breakpad_id: &str) -> Result<()> {
    let sym_name = pdb_name.replace(".pdb", ".sym");
    let target_dir = cache_dir.join(pdb_name).join(breakpad_id);
    let target_file = target_dir.join(&sym_name);

    if target_file.exists() {
        eprintln!("  [SKIP] {} (已存在)", pdb_name);
        return Ok(());
    }

    std::fs::create_dir_all(&target_dir)?;

    let url = format!(
        "{}/{}/{}/{}",
        MICROSOFT_SYMBOL_SERVER, pdb_name, breakpad_id, pdb_name
    );

    eprintln!("  [DOWNLOAD] {} ...", pdb_name);
    let resp = reqwest::get(&url)
        .await
        .with_context(|| format!("下载失败: {}", url))?;

    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}: 符号可能不存在", resp.status());
    }

    let pdb_data = resp.bytes().await?;
    if pdb_data.is_empty() {
        anyhow::bail!("下载的文件为空");
    }

    let tmp_pdb = target_dir.join(pdb_name);
    std::fs::write(&tmp_pdb, &pdb_data)?;

    eprintln!("  [CONVERT] {} ...", pdb_name);
    let output = Command::new("dump_syms")
        .arg(&tmp_pdb)
        .output()
        .with_context(|| "执行 dump_syms 失败，请确认已安装: cargo install dump_syms")?;

    let _ = std::fs::remove_file(&tmp_pdb);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("dump_syms 执行失败: {}", stderr);
    }

    std::fs::write(&target_file, &output.stdout)?;
    eprintln!("  [OK] {}", target_file.display());
    Ok(())
}

pub async fn download_missing_symbols(
    modules: &MinidumpModuleList,
    symbols_dir: &Path,
    cache_dir: &Path,
    pdb_dir: Option<&Path>,
    include_remote: bool,
) -> Result<()> {
    let symbols_dir = symbols_dir.to_path_buf();
    let cache_dir = cache_dir.to_path_buf();
    let pdb_dir = pdb_dir.map(|p| p.to_path_buf());

    let mut tasks = tokio::task::JoinSet::new();
    let mut total = 0u32;
    let mut skipped = 0u32;

    for m in modules.iter() {
        let Some(debug_file) = m.debug_file() else { continue };
        let Some(debug_id) = m.debug_identifier() else { continue };

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
            continue;
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
                    let result = dump_syms_convert(&target_dir, &local_pdb, &name);
                    match &result {
                        Ok(()) => eprintln!(
                            "  [OK] {}/{}/{}.sym",
                            name,
                            bid,
                            name.replace(".pdb", "")
                        ),
                        Err(e) => eprintln!("  [FAIL] {}: {}", name, e),
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
                let result = download_and_convert(&cache, &name, &bid).await;
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
                    eprintln!("  [FAIL] {}: {}", pdb_name, e);
                    fail += 1;
                }
            }
        }
    }

    eprintln!(
        "\n符号获取完成: 总计={}, 成功={}, 跳过(已存在)={}, 失败={}",
        total, ok, skipped, fail
    );
    Ok(())
}
