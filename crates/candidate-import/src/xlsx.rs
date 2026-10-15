//! XLSX: чтение первого листа без проверки контрольных сумм архива.
//!
//! Файлы, собранные нестандартными инструментами, часто содержат неверный CRC или нули
//! в заголовках, поэтому архив читается по центральному каталогу, а контрольные суммы
//! не проверяются.
use crate::{Grid, ImportError};

/// Предел распакованного размера записи: защита от архивов-бомб.
const MAX_ENTRY_BYTES: usize = 32 * 1024 * 1024;

fn unreadable(why: &str) -> ImportError {
    ImportError::Unreadable(why.to_string())
}

fn u16_at(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn u32_at(b: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize)
}

struct Entry {
    name: String,
    method: usize,
    compressed: usize,
    local_offset: usize,
}

/// Записи центрального каталога архива.
fn entries(zip: &[u8]) -> Result<Vec<Entry>, ImportError> {
    const EOCD: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
    let start = zip.len().saturating_sub(22 + 0xFFFF);
    let eocd = (start..zip.len().saturating_sub(21))
        .rev()
        .find(|&i| zip[i..].starts_with(&EOCD))
        .ok_or_else(|| unreadable("это не архив XLSX"))?;
    let count = u16_at(zip, eocd + 10).ok_or_else(|| unreadable("повреждённый архив"))?;
    let mut at = u32_at(zip, eocd + 16).ok_or_else(|| unreadable("повреждённый архив"))?;

    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if zip.get(at..at + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err(unreadable("повреждённый каталог архива"));
        }
        let bad = || unreadable("повреждённый каталог архива");
        let name_len = u16_at(zip, at + 28).ok_or_else(bad)?;
        let extra_len = u16_at(zip, at + 30).ok_or_else(bad)?;
        let comment_len = u16_at(zip, at + 32).ok_or_else(bad)?;
        let name = zip
            .get(at + 46..at + 46 + name_len)
            .ok_or_else(bad)
            .map(|n| String::from_utf8_lossy(n).into_owned())?;
        out.push(Entry {
            name,
            method: u16_at(zip, at + 10).ok_or_else(bad)?,
            compressed: u32_at(zip, at + 20).ok_or_else(bad)?,
            local_offset: u32_at(zip, at + 42).ok_or_else(bad)?,
        });
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

fn read_entry(zip: &[u8], e: &Entry) -> Result<Vec<u8>, ImportError> {
    let bad = || unreadable("повреждённая запись архива");
    let at = e.local_offset;
    if zip.get(at..at + 4) != Some(&[0x50, 0x4b, 0x03, 0x04]) {
        return Err(bad());
    }
    let start =
        at + 30 + u16_at(zip, at + 26).ok_or_else(bad)? + u16_at(zip, at + 28).ok_or_else(bad)?;
    let data = zip.get(start..start + e.compressed).ok_or_else(bad)?;
    match e.method {
        0 => Ok(data.to_vec()),
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, MAX_ENTRY_BYTES)
            .map_err(|_| unreadable("не удалось распаковать данные книги")),
        _ => Err(unreadable("неподдерживаемое сжатие в архиве")),
    }
}

fn text_of(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

// ---------- XML ----------

/// Содержимое элемента `<tag ...>…</tag>` в порядке следования; самозакрывающиеся дают пустую строку.
fn elements<'a>(xml: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = xml[from..].find(&open) {
        let start = from + rel;
        let after = start + open.len();
        // имя тега должно заканчиваться: `<c ` и `<c>` подходят, `<cols` — нет
        match xml.as_bytes().get(after) {
            Some(b'>') | Some(b' ') | Some(b'/') | Some(b'\t') | Some(b'\r') | Some(b'\n') => {}
            _ => {
                from = after;
                continue;
            }
        }
        let Some(tag_end) = xml[after..].find('>') else {
            break;
        };
        let attrs_end = after + tag_end;
        let attrs = &xml[after..attrs_end];
        if attrs.ends_with('/') {
            out.push((attrs, ""));
            from = attrs_end + 1;
            continue;
        }
        let body_start = attrs_end + 1;
        let Some(end_rel) = xml[body_start..].find(&close) else {
            break;
        };
        out.push((attrs, &xml[body_start..body_start + end_rel]));
        from = body_start + end_rel + close.len();
    }
    out
}

fn attr<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = attrs.find(&key)? + key.len();
    let len = attrs[start..].find('"')?;
    Some(&attrs[start..start + len])
}

fn decode_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else { break };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e if e.starts_with("#x") || e.starts_with("#X") => u32::from_str_radix(&e[2..], 16)
                .ok()
                .and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Весь текст из `<t>` внутри фрагмента (учитывает составные фрагменты `<r><t>…</t></r>`,
/// пропуская фонетические подсказки `<rPh>`).
fn texts(fragment: &str) -> String {
    let cleaned = strip_elements(fragment, "rPh");
    elements(&cleaned, "t")
        .into_iter()
        .map(|(_, body)| decode_entities(body))
        .collect()
}

fn strip_elements(xml: &str, tag: &str) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::new();
    let mut rest = xml;
    while let Some(i) = rest.find(&open) {
        out.push_str(&rest[..i]);
        match rest[i..].find(&close) {
            Some(j) => rest = &rest[i + j + close.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

fn column_index(cell_ref: &str) -> Option<usize> {
    let letters: Vec<u8> = cell_ref
        .bytes()
        .take_while(u8::is_ascii_alphabetic)
        .map(|b| b.to_ascii_uppercase())
        .collect();
    if letters.is_empty() {
        return None;
    }
    let n = letters
        .iter()
        .fold(0usize, |acc, b| acc * 26 + (b - b'A' + 1) as usize);
    Some(n - 1)
}

/// Число без экспоненты: `2.1E5` -> `210000`, `12.5` остаётся как есть.
fn number(raw: &str) -> String {
    let raw = raw.trim();
    match raw.parse::<f64>() {
        Ok(v) if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15 => format!("{}", v as i64),
        Ok(v) if v.is_finite() && (raw.contains('e') || raw.contains('E')) => format!("{v}"),
        _ => raw.to_string(),
    }
}

fn shared_strings(xml: &str) -> Vec<String> {
    elements(xml, "si")
        .into_iter()
        .map(|(_, body)| texts(body))
        .collect()
}

fn sheet_number(name: &str) -> Option<usize> {
    name.strip_prefix("xl/worksheets/sheet")?
        .strip_suffix(".xml")?
        .parse()
        .ok()
}

/// Читает первый лист книги.
pub fn read_grid(bytes: &[u8]) -> Result<Grid, ImportError> {
    let entries = entries(bytes)?;
    let shared = match entries.iter().find(|e| e.name == "xl/sharedStrings.xml") {
        Some(e) => shared_strings(&text_of(read_entry(bytes, e)?)),
        None => Vec::new(),
    };
    let sheet = entries
        .iter()
        .filter_map(|e| sheet_number(&e.name).map(|n| (n, e)))
        .min_by_key(|(n, _)| *n)
        .map(|(_, e)| e)
        .ok_or_else(|| unreadable("в книге нет листов"))?;
    let xml = text_of(read_entry(bytes, sheet)?);

    let mut rows: Grid = Vec::new();
    for (_, row_body) in elements(&xml, "row") {
        let mut row: Vec<String> = Vec::new();
        for (position, (attrs, body)) in elements(row_body, "c").into_iter().enumerate() {
            let col = attr(attrs, "r").and_then(column_index).unwrap_or(position);
            let value = match attr(attrs, "t") {
                Some("s") => elements(body, "v")
                    .first()
                    .and_then(|(_, v)| v.trim().parse::<usize>().ok())
                    .and_then(|i| shared.get(i).cloned())
                    .unwrap_or_default(),
                Some("inlineStr") => elements(body, "is")
                    .first()
                    .map(|(_, is)| texts(is))
                    .unwrap_or_default(),
                Some("str") => elements(body, "v")
                    .first()
                    .map(|(_, v)| decode_entities(v))
                    .unwrap_or_default(),
                Some("e") => String::new(),
                _ => elements(body, "v")
                    .first()
                    .map(|(_, v)| number(&decode_entities(v)))
                    .unwrap_or_default(),
            };
            if row.len() <= col {
                row.resize(col + 1, String::new());
            }
            row[col] = value.trim().to_string();
        }
        if row.iter().any(|c| !c.is_empty()) {
            rows.push(row);
        }
    }
    if rows.len() < 2 {
        return Err(ImportError::NoData);
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    for r in &mut rows {
        r.resize(width, String::new());
    }
    Ok(rows)
}
