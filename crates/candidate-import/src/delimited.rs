//! CSV и TSV: разделитель определяется по первой строке, кавычки по RFC 4180.
use crate::{Grid, ImportError};

const CANDIDATES: [char; 3] = [';', ',', '\t'];

/// Разделитель с наибольшим числом вхождений в заголовке вне кавычек (при равенстве — по порядку
/// `;`, `,`, табуляция: в русскоязычном Excel CSV разделяется точкой с запятой).
fn detect_delimiter(text: &str) -> char {
    let mut counts = [0usize; 3];
    let mut in_quotes = false;
    for c in text.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            '\n' if !in_quotes => break,
            _ if !in_quotes => {
                if let Some(i) = CANDIDATES.iter().position(|d| *d == c) {
                    counts[i] += 1;
                }
            }
            _ => {}
        }
    }
    let best = counts.iter().enumerate().max_by_key(|(i, n)| (**n, 3 - *i));
    match best {
        Some((i, n)) if *n > 0 => CANDIDATES[i],
        _ => ';',
    }
}

/// Разбирает текст в таблицу: заголовок и строки данных одинаковой ширины.
pub fn parse(text: &str) -> Result<Grid, ImportError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let delimiter = detect_delimiter(text);

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();

    let mut finish_cell = |row: &mut Vec<String>, cell: &mut String| {
        row.push(cell.trim().to_string());
        cell.clear();
    };

    while let Some(c) = chars.next() {
        if in_quotes {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    cell.push('"');
                }
                '"' => in_quotes = false,
                other => cell.push(other),
            }
            continue;
        }
        match c {
            '"' => in_quotes = true,
            c if c == delimiter => finish_cell(&mut row, &mut cell),
            '\r' => {}
            '\n' => {
                finish_cell(&mut row, &mut cell);
                rows.push(std::mem::take(&mut row));
            }
            other => cell.push(other),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        finish_cell(&mut row, &mut cell);
        rows.push(row);
    }

    rows.retain(|r| r.iter().any(|c| !c.is_empty()));
    if rows.len() < 2 {
        return Err(ImportError::NoData);
    }
    let width = rows[0].len();
    for r in &mut rows {
        r.resize(width, String::new());
    }
    Ok(rows)
}
