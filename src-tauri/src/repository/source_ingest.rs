//! 共享文件提取工具（DEV-0059 §10/§13.2）。
//!
//! - `list_zip_entries` / `read_zip_entry`：手写 ZIP 解析（local header + central directory
//!   兼容 data descriptor），复用 miniz_oxide 解压（不引入新 ZIP 依赖）。
//! - `extract_xlsx_text`：最小 XLSX 文本抽取（workbook/sharedStrings/worksheets），
//!   按 sheet 输出可读结构化文本；无法解析的特性明确报 NOT SUPPORTED。
//!
//! 禁止复制第二份 DOCX/PDF parser；DOCX/PDF 继续由 repository/personalization.rs 提供。

use std::io::Read as _;
use std::path::Path;

fn inflate_raw(data: &[u8]) -> Result<Vec<u8>, ()> {
    miniz_oxide::inflate::decompress_to_vec(data).map_err(|_| ())
}

/// 解析 ZIP 容器 → (entry 名, 原始字节) 列表（支持 local header 与 central directory；
/// 兼容 data descriptor：以 central directory 的 csize 为准）。
pub fn list_zip_entries(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    if bytes.len() < 22 || &bytes[..2] != b"PK" {
        return Err("不是有效的 ZIP 容器".to_string());
    }
    // 定位 End of Central Directory（EOCD：PK\x05\x06）
    let mut eocd: Option<usize> = None;
    let scan_end = bytes.len().saturating_sub(22);
    let scan_start = scan_end.saturating_sub(65_535);
    let mut pos = scan_end;
    while pos >= scan_start {
        if &bytes[pos..pos + 4] == b"PK\x05\x06" {
            eocd = Some(pos);
            break;
        }
        pos = pos.wrapping_sub(1);
        if pos == usize::MAX {
            break;
        }
    }
    let eocd = eocd.ok_or("ZIP 缺少 End of Central Directory".to_string())?;
    let cd_size = u32::from_le_bytes([
        bytes[eocd + 12], bytes[eocd + 13], bytes[eocd + 14], bytes[eocd + 15],
    ]) as usize;
    let cd_offset = u32::from_le_bytes([
        bytes[eocd + 16], bytes[eocd + 17], bytes[eocd + 18], bytes[eocd + 19],
    ]) as usize;
    let cd_end = cd_offset.checked_add(cd_size).ok_or("ZIP 目录越界".to_string())?;
    if cd_end > bytes.len() {
        return Err("ZIP 目录越界".to_string());
    }

    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut p = cd_offset;
    while p + 46 <= cd_end {
        if &bytes[p..p + 4] != b"PK\x01\x02" {
            return Err("ZIP 中央目录解析失败".to_string());
        }
        let method = u16::from_le_bytes([bytes[p + 10], bytes[p + 11]]);
        let csize = u32::from_le_bytes([bytes[p + 20], bytes[p + 21], bytes[p + 22], bytes[p + 23]]) as usize;
        let nlen = u16::from_le_bytes([bytes[p + 28], bytes[p + 29]]) as usize;
        let elen = u16::from_le_bytes([bytes[p + 30], bytes[p + 31]]) as usize;
        let clen = u16::from_le_bytes([bytes[p + 32], bytes[p + 33]]) as usize;
        let lho = u32::from_le_bytes([bytes[p + 42], bytes[p + 43], bytes[p + 44], bytes[p + 45]]) as usize;
        let name_bytes = &bytes[p + 46..p + 46 + nlen];
        let name = String::from_utf8_lossy(name_bytes).to_string();
        // 跳过目录条目
        if name.ends_with('/') || name.is_empty() {
            p = p + 46 + nlen + elen + clen;
            continue;
        }
        // local header：读取 name+extra，再取 csize 字节数据
        if lho + 30 <= bytes.len() && &bytes[lho..lho + 4] == b"PK\x03\x04" {
            let l_nlen = u16::from_le_bytes([bytes[lho + 26], bytes[lho + 27]]) as usize;
            let l_elen = u16::from_le_bytes([bytes[lho + 28], bytes[lho + 29]]) as usize;
            let data_start = lho + 30 + l_nlen + l_elen;
            let data_end = data_start.checked_add(csize).ok_or("ZIP 数据越界".to_string())?;
            if data_end > bytes.len() {
                return Err("ZIP 数据越界".to_string());
            }
            let raw = &bytes[data_start..data_end];
            let content = match method {
                0 => raw.to_vec(),
                8 => inflate_raw(raw).map_err(|_| format!("解压失败：{name}"))?,
                m => return Err(format!("不支持的 ZIP 压缩方式 {m}：{name}（NOT SUPPORTED）")),
            };
            entries.push((name, content));
        }
        p = p + 46 + nlen + elen + clen;
    }
    Ok(entries)
}

/// 读取指定 entry 的原始字节（不存在 → None）。
pub fn read_zip_entry(bytes: &[u8], name: &str) -> Result<Option<Vec<u8>>, String> {
    let entries = list_zip_entries(bytes)?;
    Ok(entries.into_iter().find(|(n, _)| n == name).map(|(_, c)| c))
}

fn read_file_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("打开文件失败：{e}"))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| format!("读取文件失败：{e}"))?;
    Ok(buf)
}

/// 最小 XLSX 文本抽取：workbook（sheet 名）→ sharedStrings → 每个 worksheet 单元格值，
/// 按 sheet 输出"Sheet: <名>\n<坐标>:<值>"式可读结构化文本。
/// 无法解析的特性（如不支持的 sheet 引用）明确报 NOT SUPPORTED，不 silently empty。
pub fn extract_xlsx_text(path: &Path) -> Result<String, String> {
    let bytes = read_file_bytes(path)?;
    if bytes.len() < 4 || &bytes[..2] != b"PK" {
        return Err("这不是有效的 .xlsx 文件（旧版 .xls 请先转换为 .xlsx / .csv）".to_string());
    }
    let entries = list_zip_entries(&bytes)?;
    let get = |name: &str| entries.iter().find(|(n, _)| n == name).map(|(_, c)| c.as_slice());

    // 1) workbook.xml：sheet 名 + r:id
    let workbook_xml = get("xl/workbook.xml").ok_or("xlsx 缺少 xl/workbook.xml（NOT SUPPORTED）".to_string())?;
    let workbook_text = String::from_utf8_lossy(workbook_xml).to_string();
    let mut sheet_names: Vec<(String, String)> = Vec::new(); // (rId, name)
    {
        let mut reader = quick_xml::Reader::from_str(&workbook_text);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Start(e)) if e.name().as_ref() == b"sheet" => {
                    let mut nm: Option<String> = None;
                    let mut rid: Option<String> = None;
                    for attr in e.attributes().flatten() {
                        let k = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let v = String::from_utf8_lossy(&attr.value).to_string();
                        if k == "name" {
                            nm = Some(v);
                        } else if k == "r:id" {
                            rid = Some(v);
                        }
                    }
                    if let (Some(n), Some(r)) = (nm, rid) {
                        sheet_names.push((r, n));
                    }
                }
                Ok(quick_xml::events::Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }
    }

    // 2) workbook.xml.rels：rId → target（sheetN.xml 路径）
    let mut rid_target: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(rels_xml) = get("xl/_rels/workbook.xml.rels") {
        let rels_text = String::from_utf8_lossy(rels_xml).to_string();
        let mut reader = quick_xml::Reader::from_str(&rels_text);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Start(e)) if e.name().as_ref() == b"Relationship" => {
                    let mut id = None; let mut target = None;
                    for attr in e.attributes().flatten() {
                        let k = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let v = String::from_utf8_lossy(&attr.value).to_string();
                        if k == "Id" { id = Some(v); }
                        else if k == "Target" { target = Some(v); }
                    }
                    if let (Some(i), Some(t)) = (id, target) {
                        rid_target.insert(i, t);
                    }
                }
                Ok(quick_xml::events::Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }
    }

    // 3) sharedStrings.xml
    let mut shared: Vec<String> = Vec::new();
    if let Some(ss_xml) = get("xl/sharedStrings.xml") {
        let ss_text = String::from_utf8_lossy(ss_xml).to_string();
        let mut reader = quick_xml::Reader::from_str(&ss_text);
        let mut buf = Vec::new();
        let mut in_si = false;
        let mut text_parts: Vec<String> = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(quick_xml::events::Event::Start(e)) => {
                    let name = e.name();
                    if name.as_ref() == b"si" { in_si = true; text_parts.clear(); }
                }
                Ok(quick_xml::events::Event::Text(t)) => {
                    if in_si { text_parts.push(String::from_utf8_lossy(&t).to_string()); }
                }
                Ok(quick_xml::events::Event::End(e)) => {
                    let name = e.name();
                    if name.as_ref() == b"si" {
                        shared.push(text_parts.join(""));
                        text_parts.clear();
                    }
                }
                Ok(quick_xml::events::Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }
    }

    // 4) 每个 worksheet 输出单元格值
    let mut out: Vec<String> = Vec::new();
    for (rid, name) in &sheet_names {
        let target = rid_target.get(rid).cloned().unwrap_or_default();
        let entry_name = normalize_sheet_target(&target);
        let Some(sheet_xml) = get(&entry_name) else {
            out.push(format!("Sheet: {name}（NOT SUPPORTED：找不到 {entry_name}）"));
            continue;
        };
        let sheet_text = String::from_utf8_lossy(sheet_xml).to_string();
        let rows = parse_sheet_cells(&sheet_text, &shared)?;
        out.push(format!("Sheet: {name}"));
        out.push(rows);
    }
    if out.is_empty() {
        return Err("xlsx 没有任何可解析的 sheet（NOT SUPPORTED）".to_string());
    }
    Ok(out.join("\n\n"))
}

fn normalize_sheet_target(target: &str) -> String {
    if target.starts_with('/') {
        target.trim_start_matches('/').to_string()
    } else {
        format!("xl/{target}")
    }
}

/// 解析 worksheet XML → "A1: 值" 行（共享字符串 / 内联字符串 / 数字 / 公式缓存值）。
fn parse_sheet_cells(sheet_xml: &str, shared: &[String]) -> Result<String, String> {
    let mut reader = quick_xml::Reader::from_str(sheet_xml);
    let mut buf = Vec::new();
    let mut out: Vec<String> = Vec::new();
    let mut cur_ref: Option<String> = None;
    let mut cur_type: Option<String> = None;
    let mut cur_value = String::new();
    let mut in_v = false;
    let mut in_is_t = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                let name = e.name();
                let tag = name.as_ref();
                if tag == b"c" {
                    cur_ref = None; cur_type = None; cur_value.clear();
                    for attr in e.attributes().flatten() {
                        let k = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let v = String::from_utf8_lossy(&attr.value).to_string();
                        if k == "r" { cur_ref = Some(v); }
                        else if k == "t" { cur_type = Some(v); }
                    }
                } else if tag == b"v" {
                    in_v = true;
                } else if tag == b"is" {
                    in_is_t = true;
                }
            }
            Ok(quick_xml::events::Event::Text(t)) => {
                let txt = String::from_utf8_lossy(&t).to_string();
                if in_v { cur_value.push_str(&txt); }
                else if in_is_t { cur_value.push_str(&txt); }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                let name = e.name();
                let tag = name.as_ref();
                if tag == b"v" {
                    in_v = false;
                } else if tag == b"is" {
                    in_is_t = false;
                } else if tag == b"c" {
                    if let Some(r) = cur_ref.take() {
                        let val = match cur_type.as_deref() {
                            Some("s") => cur_value
                                .parse::<usize>()
                                .ok()
                                .and_then(|i| shared.get(i))
                                .cloned()
                                .unwrap_or_else(|| "<无效共享字符串索引>".to_string()),
                            Some("inlineStr") | Some("str") => cur_value.clone(),
                            _ => cur_value.clone(),
                        };
                        let val = val.trim().to_string();
                        if !val.is_empty() {
                            out.push(format!("{r}: {val}"));
                        }
                    }
                    cur_value.clear();
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    if out.is_empty() {
        return Ok("（空 sheet）".to_string());
    }
    Ok(out.join("\n"))
}

/// 读取指定 entry 的原始字节（文件路径版；不存在 → None）。
pub fn read_zip_entry_from_path(path: &Path, name: &str) -> Result<Option<Vec<u8>>, String> {
    let bytes = read_file_bytes(path)?;
    read_zip_entry(&bytes, name)
}
