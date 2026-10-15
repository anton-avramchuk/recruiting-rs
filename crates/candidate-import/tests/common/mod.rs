#![allow(dead_code)]
//! Сборка маленьких архивов для тестов: записи без сжатия либо со сжатием Deflate.

pub struct Entry<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
    pub deflate: bool,
    /// Записать заведомо неверную контрольную сумму.
    pub bad_crc: bool,
}

pub fn entry<'a>(name: &'a str, data: &'a [u8], deflate: bool) -> Entry<'a> {
    Entry {
        name,
        data,
        deflate,
        bad_crc: true,
    }
}

fn u16le(v: u16) -> [u8; 2] {
    v.to_le_bytes()
}

fn u32le(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

pub fn zip(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for e in entries {
        let (method, body) = if e.deflate {
            (8u16, miniz_oxide::deflate::compress_to_vec(e.data, 6))
        } else {
            (0u16, e.data.to_vec())
        };
        let offset = out.len() as u32;
        let crc = if e.bad_crc { 0 } else { 0xDEAD_BEEF };
        out.extend(u32le(0x0403_4b50));
        out.extend(u16le(20));
        out.extend(u16le(0));
        out.extend(u16le(method));
        out.extend(u16le(0));
        out.extend(u16le(0));
        out.extend(u32le(crc));
        out.extend(u32le(body.len() as u32));
        out.extend(u32le(e.data.len() as u32));
        out.extend(u16le(e.name.len() as u16));
        out.extend(u16le(0));
        out.extend(e.name.as_bytes());
        out.extend(&body);

        central.extend(u32le(0x0201_4b50));
        central.extend(u16le(20));
        central.extend(u16le(20));
        central.extend(u16le(0));
        central.extend(u16le(method));
        central.extend(u16le(0));
        central.extend(u16le(0));
        central.extend(u32le(crc));
        central.extend(u32le(body.len() as u32));
        central.extend(u32le(e.data.len() as u32));
        central.extend(u16le(e.name.len() as u16));
        central.extend(u16le(0));
        central.extend(u16le(0));
        central.extend(u16le(0));
        central.extend(u16le(0));
        central.extend(u32le(0));
        central.extend(u32le(offset));
        central.extend(e.name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    out.extend(&central);
    out.extend(u32le(0x0605_4b50));
    out.extend(u16le(0));
    out.extend(u16le(0));
    out.extend(u16le(entries.len() as u16));
    out.extend(u16le(entries.len() as u16));
    out.extend(u32le(central.len() as u32));
    out.extend(u32le(cd_offset));
    out.extend(u16le(0));
    out
}

/// Книга с одним листом: `sheet` — содержимое `<sheetData>`, `strings` — общие строки.
pub fn workbook(sheet_rows: &str, strings: &[&str], deflate: bool) -> Vec<u8> {
    let sst: String = strings
        .iter()
        .map(|s| format!("<si><t>{s}</t></si>"))
        .collect();
    let shared = format!(r#"<?xml version="1.0"?><sst>{sst}</sst>"#);
    let sheet = format!(r#"<?xml version="1.0"?><worksheet><sheetData>{sheet_rows}</sheetData></worksheet>"#);
    zip(&[
        entry("xl/sharedStrings.xml", shared.as_bytes(), deflate),
        entry("xl/worksheets/sheet1.xml", sheet.as_bytes(), deflate),
    ])
}
