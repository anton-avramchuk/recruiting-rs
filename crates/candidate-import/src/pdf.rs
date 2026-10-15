//! PDF-резюме: извлечение текста из простых шрифтов (WinAnsi и `Differences`).
//!
//! Это не полноценный разборщик PDF: читаются потоки содержимого страниц (в том числе сжатые Flate),
//! текстовые операторы и таблица `Differences` с названиями глифов. PDF со шрифтами-подмножествами
//! (Identity-H без таблицы Unicode) дают нечитаемый текст — это распознаёт [`readable`], и
//! интерфейс предлагает вставить текст резюме вручную.
use std::collections::HashMap;

use crate::ImportError;

/// Предел распакованного размера одного потока.
const MAX_STREAM_BYTES: usize = 32 * 1024 * 1024;
/// Доля «осмысленных» символов, начиная с которой текст считается читаемым.
const READABLE_RATIO: f64 = 0.85;
/// Разрыв в массиве `TJ` (в тысячных долях em), с которого считаем, что между словами пробел.
const WORD_GAP: f64 = -200.0;

fn unreadable(why: &str) -> ImportError {
    ImportError::Unreadable(why.to_string())
}

// ---------- поиск байтовых последовательностей ----------

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= hay.len() || needle.is_empty() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| from + i)
}

fn rfind(hay: &[u8], needle: &[u8], before: usize) -> Option<usize> {
    let end = before.min(hay.len());
    if needle.is_empty() || end < needle.len() {
        return None;
    }
    hay[..end].windows(needle.len()).rposition(|w| w == needle)
}

// ---------- кодировка ----------

const CP1252_HIGH: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

fn cp1252(byte: u8) -> char {
    match byte {
        0x80..=0x9f => CP1252_HIGH[(byte - 0x80) as usize],
        b => b as char,
    }
}

/// Юникод по названию глифа из списка Adobe (кириллица `afii100xx`, `uniXXXX`, часть латиницы и знаков).
fn glyph_char(name: &str) -> Option<char> {
    if let Some(num) = name
        .strip_prefix("afii")
        .and_then(|n| n.parse::<u32>().ok())
    {
        let (base, yo_at, from) = match num {
            10017..=10049 => (0x410, 6, 10017),
            10065..=10097 => (0x430, 6, 10065),
            _ => return None,
        };
        let i = num - from;
        let code = match i {
            _ if i == yo_at => {
                if base == 0x410 {
                    0x401
                } else {
                    0x451
                }
            }
            _ if i < yo_at => base + i,
            _ => base + i - 1,
        };
        return char::from_u32(code);
    }
    if let Some(hex) = name.strip_prefix("uni").filter(|h| h.len() == 4) {
        return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
    }
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && c.is_ascii_alphanumeric()
    {
        return Some(c);
    }
    Some(match name {
        "space" => ' ',
        "period" => '.',
        "comma" => ',',
        "hyphen" | "minus" => '-',
        "endash" => '–',
        "emdash" => '—',
        "colon" => ':',
        "semicolon" => ';',
        "at" => '@',
        "plus" => '+',
        "parenleft" => '(',
        "parenright" => ')',
        "underscore" => '_',
        "slash" => '/',
        "percent" => '%',
        "numbersign" => '#',
        "quotedbl" => '"',
        "quotesingle" => '\'',
        "exclam" => '!',
        "question" => '?',
        "zero" => '0',
        "one" => '1',
        "two" => '2',
        "three" => '3',
        "four" => '4',
        "five" => '5',
        "six" => '6',
        "seven" => '7',
        "eight" => '8',
        "nine" => '9',
        _ => return None,
    })
}

/// Таблица `Differences` первой кодировки в файле: код символа -> символ.
fn differences(pdf: &[u8]) -> HashMap<u8, char> {
    let mut map = HashMap::new();
    let Some(at) = find(pdf, b"/Differences", 0) else {
        return map;
    };
    let Some(open) = find(pdf, b"[", at) else {
        return map;
    };
    let Some(close) = find(pdf, b"]", open) else {
        return map;
    };
    let body = String::from_utf8_lossy(&pdf[open + 1..close]);
    let mut code: i64 = 0;
    for token in body.split_whitespace() {
        if let Some(name) = token.strip_prefix('/') {
            if let (Ok(c), Some(ch)) = (u8::try_from(code), glyph_char(name)) {
                map.insert(c, ch);
            }
            code += 1;
        } else if let Ok(n) = token.parse::<i64>() {
            code = n;
        }
    }
    map
}

// ---------- потоки ----------

/// Потоки содержимого, похожие на текстовые (внутри есть `BT … ET`), распакованные.
fn content_streams(pdf: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = find(pdf, b"stream", from) {
        from = at + 6;
        // ключевое слово `stream` стоит после словаря `>>`, а не внутри `endstream`
        let before = &pdf[..at];
        let trimmed = before
            .iter()
            .rposition(|b| !b.is_ascii_whitespace())
            .map(|i| &before[..=i])
            .unwrap_or(&[]);
        if !trimmed.ends_with(b">>") {
            continue;
        }
        let mut start = at + 6;
        if pdf.get(start) == Some(&b'\r') {
            start += 1;
        }
        if pdf.get(start) == Some(&b'\n') {
            start += 1;
        }
        let Some(end) = find(pdf, b"endstream", start) else {
            break;
        };
        from = end + 9;
        let dict_start = rfind(pdf, b"obj", at).unwrap_or(0);
        let dict = &pdf[dict_start..at];
        let mut data = &pdf[start..end];
        while data.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
            data = &data[..data.len() - 1];
        }
        let body = if find(dict, b"/FlateDecode", 0).is_some() {
            match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, MAX_STREAM_BYTES) {
                Ok(v) => v,
                Err(_) => continue,
            }
        } else if find(dict, b"/Filter", 0).is_some() {
            continue; // другие фильтры не поддерживаются
        } else {
            data.to_vec()
        };
        if find(&body, b"BT", 0).is_some() && find(&body, b"ET", 0).is_some() {
            out.push(body);
        }
    }
    out
}

// ---------- операторы текста ----------

enum Operand {
    Number(f64),
    Str(Vec<u8>),
    Array(Vec<Operand>),
}

struct Writer<'a> {
    out: String,
    map: &'a HashMap<u8, char>,
}

impl Writer<'_> {
    fn text(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.out
                .push(self.map.get(b).copied().unwrap_or_else(|| cp1252(*b)));
        }
    }

    fn newline(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    fn space(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with([' ', '\n']) {
            self.out.push(' ');
        }
    }
}

fn is_delimiter(b: u8) -> bool {
    b.is_ascii_whitespace() || matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'/' | b'%')
}

/// Строка в круглых скобках с экранированием; `at` указывает на `(`. Возвращает байты и позицию после `)`.
fn literal(data: &[u8], at: usize) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut i = at;
    while i < data.len() {
        let b = data[i];
        i += 1;
        match b {
            b'(' => {
                if depth > 0 {
                    out.push(b);
                }
                depth += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return (out, i);
                }
                out.push(b);
            }
            b'\\' => {
                let Some(&n) = data.get(i) else { break };
                i += 1;
                match n {
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'b' => out.push(0x08),
                    b'f' => out.push(0x0c),
                    b'\r' => {
                        if data.get(i) == Some(&b'\n') {
                            i += 1;
                        }
                    }
                    b'\n' => {}
                    b'0'..=b'7' => {
                        let mut value = (n - b'0') as u32;
                        for _ in 0..2 {
                            match data.get(i) {
                                Some(d @ b'0'..=b'7') => {
                                    value = value * 8 + (d - b'0') as u32;
                                    i += 1;
                                }
                                _ => break,
                            }
                        }
                        out.push((value & 0xff) as u8);
                    }
                    other => out.push(other),
                }
            }
            other => out.push(other),
        }
    }
    (out, i)
}

/// Шестнадцатеричная строка; `at` указывает на `<`.
fn hex(data: &[u8], at: usize) -> (Vec<u8>, usize) {
    let end = find(data, b">", at).unwrap_or(data.len());
    let digits: Vec<u8> = data[at + 1..end]
        .iter()
        .filter_map(|b| (*b as char).to_digit(16).map(|d| d as u8))
        .collect();
    let bytes = digits
        .chunks(2)
        .map(|p| (p[0] << 4) | p.get(1).copied().unwrap_or(0))
        .collect();
    (bytes, (end + 1).min(data.len()))
}

fn text_of_stream(data: &[u8], map: &HashMap<u8, char>) -> String {
    let mut w = Writer {
        out: String::new(),
        map,
    };
    let mut stack: Vec<Operand> = Vec::new();
    let mut array: Option<Vec<Operand>> = None;
    let mut i = 0;
    let push = |stack: &mut Vec<Operand>, array: &mut Option<Vec<Operand>>, op: Operand| match array
    {
        Some(a) => a.push(op),
        None => stack.push(op),
    };

    while i < data.len() {
        let b = data[i];
        match b {
            _ if b.is_ascii_whitespace() => i += 1,
            b'%' => {
                while i < data.len() && data[i] != b'\n' && data[i] != b'\r' {
                    i += 1;
                }
            }
            b'(' => {
                let (s, next) = literal(data, i);
                push(&mut stack, &mut array, Operand::Str(s));
                i = next;
            }
            b'<' if data.get(i + 1) == Some(&b'<') => i += 2,
            b'>' if data.get(i + 1) == Some(&b'>') => i += 2,
            b'<' => {
                let (s, next) = hex(data, i);
                push(&mut stack, &mut array, Operand::Str(s));
                i = next;
            }
            b'[' => {
                array = Some(Vec::new());
                i += 1;
            }
            b']' => {
                if let Some(a) = array.take() {
                    stack.push(Operand::Array(a));
                }
                i += 1;
            }
            b'/' => {
                i += 1;
                while i < data.len() && !is_delimiter(data[i]) {
                    i += 1;
                }
            }
            _ => {
                let start = i;
                while i < data.len() && !is_delimiter(data[i]) {
                    i += 1;
                }
                if i == start {
                    i += 1; // одиночный разделитель, который не разобран выше
                    continue;
                }
                let token = &data[start..i];
                if let Some(n) = std::str::from_utf8(token)
                    .ok()
                    .and_then(|t| t.parse::<f64>().ok())
                {
                    push(&mut stack, &mut array, Operand::Number(n));
                    continue;
                }
                match token {
                    b"Tj" => {
                        if let Some(Operand::Str(s)) = stack.last() {
                            w.text(s);
                        }
                    }
                    b"'" | b"\"" => {
                        w.newline();
                        if let Some(Operand::Str(s)) = stack.last() {
                            w.text(s);
                        }
                    }
                    b"TJ" => {
                        if let Some(Operand::Array(items)) = stack.last() {
                            for item in items {
                                match item {
                                    Operand::Str(s) => w.text(s),
                                    Operand::Number(n) if *n < WORD_GAP => w.space(),
                                    _ => {}
                                }
                            }
                        }
                    }
                    b"Td" | b"TD" => {
                        let nums: Vec<f64> = stack
                            .iter()
                            .filter_map(|o| match o {
                                Operand::Number(n) => Some(*n),
                                _ => None,
                            })
                            .collect();
                        if let [.., tx, ty] = nums[..] {
                            if ty != 0.0 {
                                w.newline();
                            } else if tx != 0.0 {
                                w.space();
                            }
                        }
                    }
                    b"T*" | b"ET" => w.newline(),
                    _ => {}
                }
                stack.clear();
            }
        }
    }
    w.out
}

/// Текст PDF-файла построчно.
pub fn extract_text(bytes: &[u8]) -> Result<String, ImportError> {
    let header = &bytes[..bytes.len().min(1024)];
    if find(header, b"%PDF-", 0).is_none() {
        return Err(unreadable("это не PDF-файл"));
    }
    let map = differences(bytes);
    let text: Vec<String> = content_streams(bytes)
        .iter()
        .map(|s| text_of_stream(s, &map))
        .filter(|t| !t.trim().is_empty())
        .collect();
    if text.is_empty() {
        return Err(unreadable("в PDF не найден текст"));
    }
    Ok(text.join("\n"))
}

/// Похож ли извлечённый текст на человеческий, а не на номера глифов.
pub fn readable(text: &str) -> bool {
    let meaningful = |c: char| {
        c.is_ascii_alphanumeric()
            || c.is_ascii_punctuation()
            || ('\u{400}'..='\u{4ff}').contains(&c)
            || "—–…«»№₽€•‘’“”".contains(c)
    };
    let mut total = 0usize;
    let mut good = 0usize;
    for c in text.chars().filter(|c| !c.is_whitespace()) {
        total += 1;
        if meaningful(c) {
            good += 1;
        }
    }
    total >= 3 && good as f64 / total as f64 >= READABLE_RATIO
}
