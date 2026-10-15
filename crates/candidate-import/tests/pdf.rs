use candidate_import::{ImportError, pdf};

fn sample() -> Vec<u8> {
    std::fs::read("tests/fixtures/demo-rezyume.pdf").unwrap()
}

/// Минимальный PDF: один поток содержимого и необязательный объект кодировки шрифта.
fn pdf_with(content: &[u8], filter: Option<&str>, encoding: Option<&str>) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    out.extend(b"1 0 obj\n<</Type/Catalog/Pages 2 0 R>>\nendobj\n");
    out.extend(b"4 0 obj\n");
    match filter {
        Some(f) => out.extend(format!("<</Length {}/Filter/{f}>>\n", content.len()).bytes()),
        None => out.extend(format!("<</Length {}>>\n", content.len()).bytes()),
    }
    out.extend(b"stream\n");
    out.extend(content);
    out.extend(b"\nendstream\nendobj\n");
    out.extend(
        b"5 0 obj\n<</Type/Font/Subtype/Type1/BaseFont/Helvetica/Encoding 6 0 R>>\nendobj\n",
    );
    if let Some(enc) = encoding {
        out.extend(format!("6 0 obj\n{enc}\nendobj\n").bytes());
    }
    out.extend(b"trailer\n<</Root 1 0 R>>\n%%EOF\n");
    out
}

fn lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

#[test]
fn sample_resume_is_read_as_russian_text() {
    let text = pdf::extract_text(&sample()).unwrap();
    assert_eq!(
        lines(&text),
        [
            "Ирина Соколова",
            "Аналитик данных",
            "Казань   +7 916 442-11-07   irina@mail.ru",
            "Ожидания: 210000 руб",
            "Опыт работы 7 лет",
            "Навыки: SQL, Python, dbt, PostgreSQL",
            "Опыт работы",
            "2022 - наст. время, Ozon, Ведущий аналитик данных",
            "2019 - 2022, X5 Tech, Аналитик",
            "2017 - 2019, Магнит, Junior-аналитик",
            "Образование",
            "2017, Казанский федеральный университет, Прикладная математика и информатика",
            "Строила витрины и систему отчетности для розничной сети, вела команду",
            "из трех аналитиков и отвечала за качество данных.",
        ]
    );
    assert!(pdf::readable(&text));
}

#[test]
fn flate_streams_are_inflated() {
    let content = b"BT /F1 12 Tf (Hello flate) Tj ET";
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(content, 6);
    let text = pdf::extract_text(&pdf_with(&packed, Some("FlateDecode"), None)).unwrap();
    assert_eq!(lines(&text), ["Hello flate"]);
}

#[test]
fn literal_string_escapes_are_decoded() {
    let content = br"BT (a\(b\)\\c\101\n) Tj 0 -10 Td (line\
cont) Tj ET";
    let text = pdf::extract_text(&pdf_with(content, None, None)).unwrap();
    assert_eq!(lines(&text), ["a(b)\\cA", "linecont"]);
}

#[test]
fn hex_strings_are_bytes() {
    let content = b"BT <48656C6C6F> Tj ET";
    let text = pdf::extract_text(&pdf_with(content, None, None)).unwrap();
    assert_eq!(lines(&text), ["Hello"]);
}

#[test]
fn kerning_gaps_in_tj_arrays_become_spaces() {
    let content = b"BT [(Hel) -20 (lo) -300 (world)] TJ ET";
    let text = pdf::extract_text(&pdf_with(content, None, None)).unwrap();
    assert_eq!(lines(&text), ["Hello world"]);
}

#[test]
fn vertical_moves_start_a_new_line_and_horizontal_ones_a_space() {
    let content = b"BT (one) Tj 40 0 Td (two) Tj 0 -14 Td (three) Tj T* (four) Tj ET";
    let text = pdf::extract_text(&pdf_with(content, None, None)).unwrap();
    assert_eq!(lines(&text), ["one two", "three", "four"]);
}

#[test]
fn winansi_bytes_outside_differences_use_cp1252() {
    let content = b"BT (caf\xe9 \x80 5) Tj ET";
    let text = pdf::extract_text(&pdf_with(content, None, None)).unwrap();
    assert_eq!(lines(&text), ["café € 5"]);
}

#[test]
fn differences_with_unicode_glyph_names_are_applied() {
    let enc = "<</Type/Encoding/BaseEncoding/WinAnsiEncoding/Differences[65 /uni0416 /afii10023 97 /afii10071]>>";
    let content = b"BT (AB a) Tj ET";
    let text = pdf::extract_text(&pdf_with(content, None, Some(enc))).unwrap();
    assert_eq!(lines(&text), ["ЖЁ ё"]);
}

#[test]
fn a_pdf_without_text_is_unreadable() {
    let err = pdf::extract_text(&pdf_with(b"0 0 m 10 10 l S", None, None)).unwrap_err();
    assert!(matches!(err, ImportError::Unreadable(_)), "{err:?}");
}

#[test]
fn not_a_pdf_is_reported() {
    for bad in [&b"hello, I am text"[..], b"", b"%PDF-1.4 but nothing else"] {
        assert!(matches!(
            pdf::extract_text(bad),
            Err(ImportError::Unreadable(_))
        ));
    }
}

#[test]
fn readability_tells_real_text_from_glyph_ids() {
    assert!(pdf::readable(
        "Ирина Соколова\nKazan, +7 916 442-11-07, irina@mail.ru"
    ));
    assert!(!pdf::readable(""));
    assert!(!pdf::readable("   \n  "));
    // шрифт Identity-H без таблицы Unicode: в потоке номера глифов
    assert!(!pdf::readable(
        "\u{1}\u{0}H\u{0}e\u{3}\u{4}\u{1f}\u{2}\u{0}l\u{5}\u{6}\u{7}\u{8}"
    ));
    assert!(!pdf::readable("ÿþØÞÐðÝÍÓÖ ÿþØÞÐðÝÍÓÖ"));
}

#[test]
fn identity_h_text_is_extracted_but_marked_unreadable() {
    let content = b"BT <00010002000300040005000600070008> Tj ET";
    let text = pdf::extract_text(&pdf_with(content, None, None)).unwrap();
    assert!(!pdf::readable(&text), "{text:?}");
}
