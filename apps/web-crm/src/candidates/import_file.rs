//! Чтение выбранного файла импорта в браузере: файл разбирается на устройстве и никуда не уходит.
use candidate_import::{Grid, delimited, pdf, xlsx};

/// Предел размера файла, байт.
pub const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;
/// Предел числа строк данных.
pub const MAX_ROWS: usize = 2000;

/// Что получилось из файла.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loaded {
    /// Таблица: первая строка — заголовок.
    Table { name: String, grid: Grid },
    /// Резюме: текст из PDF и признак того, что его можно читать.
    Resume {
        name: String,
        text: String,
        readable: bool,
    },
}

fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default()
}

/// Разбирает содержимое файла по расширению; ошибка — сообщение для пользователя.
pub fn classify_bytes(name: &str, bytes: &[u8]) -> Result<Loaded, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err("Файл больше 5 МБ: разделите его на части.".into());
    }
    let table = |grid: Result<Grid, candidate_import::ImportError>| match grid {
        Ok(grid) if grid.len() - 1 > MAX_ROWS => Err(format!(
            "В файле {} строк, а за раз можно не больше {MAX_ROWS}.",
            grid.len() - 1
        )),
        Ok(grid) => Ok(Loaded::Table {
            name: name.to_string(),
            grid,
        }),
        Err(e) => Err(format!("{e}.")),
    };
    match extension(name).as_str() {
        "csv" | "tsv" | "txt" => table(delimited::parse(&delimited::decode(bytes))),
        "xlsx" => table(xlsx::read_grid(bytes)),
        "pdf" => match pdf::extract_text(bytes) {
            Ok(text) => Ok(Loaded::Resume {
                name: name.to_string(),
                readable: pdf::readable(&text),
                text,
            }),
            // текста не нашли: разобрать нечего, остаётся ручная вставка
            Err(_) => Ok(Loaded::Resume {
                name: name.to_string(),
                text: String::new(),
                readable: false,
            }),
        },
        _ => Err("Поддерживаются файлы .csv, .tsv, .xlsx и .pdf.".into()),
    }
}

/// Текущий год по часам браузера (нужен, чтобы посчитать опыт по местам работы).
#[cfg(feature = "hydrate")]
pub fn current_year() -> i32 {
    js_sys::Date::new_0().get_full_year() as i32
}

#[cfg(not(feature = "hydrate"))]
pub fn current_year() -> i32 {
    2026
}

/// Читает файл, выбранный пользователем.
#[cfg(feature = "hydrate")]
pub async fn read_file(file: web_sys::File) -> Result<Loaded, String> {
    if file.size() as usize > MAX_FILE_BYTES {
        return Err("Файл больше 5 МБ: разделите его на части.".into());
    }
    let buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
        .await
        .map_err(|_| "Не удалось прочитать файл.".to_string())?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    classify_bytes(&file.name(), &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../../crates/candidate-import/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[test]
    fn the_extension_chooses_the_parser() {
        match classify_bytes("База.CSV", &fixture("demo-kandidaty.csv")).unwrap() {
            Loaded::Table { grid, name } => {
                assert_eq!(name, "База.CSV");
                assert_eq!(grid.len(), 3);
            }
            other => panic!("{other:?}"),
        }
        match classify_bytes("x.xlsx", &fixture("demo-kandidaty.xlsx")).unwrap() {
            Loaded::Table { grid, .. } => assert_eq!(grid[1][0], "Ирина Соколова"),
            other => panic!("{other:?}"),
        }
        match classify_bytes("cv.pdf", &fixture("demo-rezyume.pdf")).unwrap() {
            Loaded::Resume { text, readable, .. } => {
                assert!(readable);
                assert!(text.contains("Ирина Соколова"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unsupported_and_oversized_files_are_refused_with_a_message() {
        assert!(classify_bytes("a.docx", b"x").unwrap_err().contains(".csv"));
        let big = vec![b'a'; MAX_FILE_BYTES + 1];
        assert!(
            classify_bytes("big.csv", &big)
                .unwrap_err()
                .contains("5 МБ")
        );
    }

    #[test]
    fn too_many_rows_are_refused() {
        let mut csv = String::from("Имя\n");
        for i in 0..=MAX_ROWS {
            csv.push_str(&format!("Человек {i}\n"));
        }
        let err = classify_bytes("many.csv", csv.as_bytes()).unwrap_err();
        assert!(err.contains("2000"), "{err}");
    }

    #[test]
    fn a_file_without_data_says_so() {
        let err = classify_bytes("empty.csv", "Имя;Город".as_bytes()).unwrap_err();
        assert!(err.contains("нет строк с данными"), "{err}");
    }

    #[test]
    fn a_pdf_without_text_falls_back_to_manual_input() {
        match classify_bytes("scan.pdf", b"%PDF-1.4\n%%EOF").unwrap() {
            Loaded::Resume { readable, text, .. } => {
                assert!(!readable);
                assert!(text.is_empty());
            }
            other => panic!("{other:?}"),
        }
    }
}
