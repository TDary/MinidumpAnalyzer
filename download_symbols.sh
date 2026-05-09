#!/usr/bin/env bash
#
# 从微软符号服务器下载 PDB 并用 dump_syms 转换为 Breakpad .sym 文件
#
# 用法:
#   ./download_symbols.sh <symbol_cache_dir> < pdb_list.txt
#
# pdb_list.txt 每行格式: pdb文件名 debug_id(带横杠的GUID格式)
# 也可直接在 minidump-analyzer 输出中提取模块列表后使用
#
# 依赖: curl, dump_syms (需在 PATH 中)
#
# 示例:
#   cat pdb_list.txt
#   ntdll.pdb 5a9f4004-db80-6b50-bcbd-7b6c55417b04-1
#   kernel32.pdb f7b745de-7a69-ef1b-7c8b-e0c4fded10a1-1
#
#   ./download_symbols.sh D:/PDBTest/sym_cache < pdb_list.txt

set -uo pipefail

SYM_CACHE="${1:?用法: $0 <symbol_cache_dir>}"
MICROSOFT_SYMBOL_SERVER="https://msdl.microsoft.com/download/symbols"

# 将 17b50809-0f9e-4322-b8f7-492fc39e6637-4 转为 17B508090F9E4322B8F7492FC39E66374
guid_to_breakpad_id() {
    local guid="$1"
    # 移除横杠，转大写
    echo "${guid//-/}" | tr '[:lower:]' '[:upper:]'
}

download_and_convert() {
    local pdb_name="$1"
    local debug_id_guid="$2"
    local breakpad_id
    breakpad_id=$(guid_to_breakpad_id "$debug_id_guid")

    local sym_name="${pdb_name%.pdb}.sym"
    local target_dir="${SYM_CACHE}/${pdb_name}/${breakpad_id}"
    local target_file="${target_dir}/${sym_name}"

    # 已经存在则跳过 (return 2 = skip)
    if [[ -f "$target_file" ]]; then
        return 2
    fi

    mkdir -p "$target_dir"

    local tmp_pdb
    tmp_pdb=$(mktemp --suffix=.pdb) || {
        echo "[FAIL] $pdb_name: 无法创建临时文件" >&2
        return 1
    }

    local url="${MICROSOFT_SYMBOL_SERVER}/${pdb_name}/${breakpad_id}/${pdb_name}"

    echo "[DOWNLOAD] $pdb_name ..."
    if curl -sL --connect-timeout 10 --max-time 60 -o "$tmp_pdb" "$url" 2>/dev/null; then
        if [[ -f "$tmp_pdb" && -s "$tmp_pdb" ]]; then
            echo "[CONVERT] $pdb_name ..."
            if dump_syms "$tmp_pdb" > "$target_file" 2>/dev/null; then
                echo "[OK] $target_file"
            else
                echo "[FAIL] $pdb_name: dump_syms 转换失败" >&2
                rm -f "$tmp_pdb"
                return 1
            fi
        else
            echo "[FAIL] $pdb_name: 下载的文件为空 (debug_id 可能不匹配)" >&2
            rm -f "$tmp_pdb"
            return 1
        fi
    else
        echo "[FAIL] $pdb_name: 下载失败 (网络错误或符号不存在)" >&2
        rm -f "$tmp_pdb"
        return 1
    fi

    rm -f "$tmp_pdb"
    return 0
}

# 主流程: 从 stdin 读取每行 "pdb_name debug_id"
success=0
fail=0
skip=0

while IFS=' ' read -r pdb_name debug_id_guid rest || [[ -n "$pdb_name" ]]; do
    [[ -z "$pdb_name" || "$pdb_name" == \#* ]] && continue
    [[ -z "$debug_id_guid" ]] && continue

    download_and_convert "$pdb_name" "$debug_id_guid"
    rc=$?
    case $rc in
        0) ((success++)) || true ;;
        1) ((fail++)) || true ;;
        2) ((skip++)) || true ;;
    esac
done

echo ""
echo "完成: 成功=$success, 跳过(已存在)=$skip, 失败=$fail"
