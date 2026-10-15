mod common;

use candidate_import::{ImportError, delimited, xlsx};
use common::{entry, workbook, zip};

fn sample() -> Vec<u8> {
    std::fs::read("tests/fixtures/demo-kandidaty.xlsx").unwrap()
}

#[test]
fn sample_with_a_broken_crc_is_read() {
    let grid = xlsx::read_grid(&sample()).unwrap();
    assert_eq!(grid.len(), 2, "header + one row");
    assert_eq!(
        grid[0],
        ["ФИО", "Телефон", "Почта", "Город", "Должность", "Ожидания", "Навыки"]
    );
    assert_eq!(
        grid[1],
        [
            "Ирина Соколова",
            "+7 916 442-11-07",
            "irina@mail.ru",
            "Казань",
            "Аналитик данных",
            "210000",
            "SQL, Python, dbt"
        ],
        "shared strings, inline strings and a number"
    );
}

#[test]
fn stored_and_deflated_entries_give_the_same_table() {
    let rows = r#"<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row><row r="2"><c r="A2"><v>7</v></c><c r="B2" t="inlineStr"><is><t>x</t></is></c></row>"#;
    let stored = xlsx::read_grid(&workbook(rows, &["a", "b"], false)).unwrap();
    let deflated = xlsx::read_grid(&workbook(rows, &["a", "b"], true)).unwrap();
    assert_eq!(stored, deflated);
    assert_eq!(stored, vec![vec!["a", "b"], vec!["7", "x"]]);
}

#[test]
fn skipped_cells_keep_their_columns() {
    let rows = r#"<row r="1"><c r="A1" t="inlineStr"><is><t>a</t></is></c><c r="B1" t="inlineStr"><is><t>b</t></is></c><c r="C1" t="inlineStr"><is><t>c</t></is></c><c r="D1" t="inlineStr"><is><t>d</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>1</t></is></c><c r="D2" t="inlineStr"><is><t>4</t></is></c></row>"#;
    let grid = xlsx::read_grid(&workbook(rows, &[], false)).unwrap();
    assert_eq!(grid[1], ["1", "", "", "4"]);
}

#[test]
fn columns_past_z_are_addressed_by_letters() {
    let rows = r#"<row r="1"><c r="A1" t="inlineStr"><is><t>x</t></is></c><c r="AB1" t="inlineStr"><is><t>y</t></is></c></row><row r="2"><c r="AB2" t="inlineStr"><is><t>z</t></is></c></row>"#;
    let grid = xlsx::read_grid(&workbook(rows, &[], false)).unwrap();
    assert_eq!(grid[0].len(), 28);
    assert_eq!(grid[0][27], "y");
    assert_eq!(grid[1][27], "z");
}

#[test]
fn xml_entities_and_rich_text_runs_are_decoded() {
    let rows = r#"<row r="1"><c r="A1" t="inlineStr"><is><t>Том &amp; Джерри</t></is></c><c r="B1" t="inlineStr"><is><r><t>Ан</t></r><r><t>на</t></r></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>&lt;a&gt; &quot;q&quot; &#1064; &#x428;</t></is></c><c r="B2" t="str"><v>f</v></c></row>"#;
    let grid = xlsx::read_grid(&workbook(rows, &[], false)).unwrap();
    assert_eq!(grid[0], ["Том & Джерри", "Анна"]);
    assert_eq!(grid[1], ["<a> \"q\" Ш Ш", "f"]);
}

#[test]
fn numbers_are_written_without_exponents() {
    let rows = r#"<row r="1"><c r="A1" t="inlineStr"><is><t>n</t></is></c></row><row r="2"><c r="A2"><v>2.1E5</v></c></row><row r="3"><c r="A3"><v>12.5</v></c></row>"#;
    let grid = xlsx::read_grid(&workbook(rows, &[], false)).unwrap();
    assert_eq!(grid[1][0], "210000");
    assert_eq!(grid[2][0], "12.5");
}

#[test]
fn the_first_sheet_by_number_is_used() {
    let sheet = |v: &str| format!(r#"<worksheet><sheetData><row><c t="inlineStr"><is><t>h</t></is></c></row><row><c t="inlineStr"><is><t>{v}</t></is></c></row></sheetData></worksheet>"#);
    let (s2, s10, s1) = (sheet("two"), sheet("ten"), sheet("one"));
    let book = zip(&[
        entry("xl/worksheets/sheet2.xml", s2.as_bytes(), false),
        entry("xl/worksheets/sheet10.xml", s10.as_bytes(), false),
        entry("xl/worksheets/sheet1.xml", s1.as_bytes(), false),
    ]);
    assert_eq!(xlsx::read_grid(&book).unwrap()[1][0], "one");
}

#[test]
fn a_sheet_without_data_rows_is_an_error() {
    let rows = r#"<row r="1"><c r="A1" t="inlineStr"><is><t>only a header</t></is></c></row>"#;
    assert_eq!(
        xlsx::read_grid(&workbook(rows, &[], false)),
        Err(ImportError::NoData)
    );
}

#[test]
fn not_an_archive_is_reported() {
    let err = xlsx::read_grid("это текст, а не книга".as_bytes()).unwrap_err();
    assert!(matches!(err, ImportError::Unreadable(_)), "{err:?}");
    assert!(matches!(xlsx::read_grid(&[]), Err(ImportError::Unreadable(_))));
    // архив без листа
    let empty = zip(&[entry("docProps/app.xml", b"<x/>", false)]);
    assert!(matches!(xlsx::read_grid(&empty), Err(ImportError::Unreadable(_))));
}

#[test]
fn a_corrupt_deflate_stream_is_reported_not_panicked() {
    let mut book = workbook("<row/>", &[], true);
    // портим сжатые данные первой записи
    for b in book.iter_mut().skip(40).take(12) {
        *b = 0xFF;
    }
    assert!(matches!(xlsx::read_grid(&book), Err(ImportError::Unreadable(_))));
}

#[test]
fn xlsx_and_csv_of_the_same_data_agree() {
    let csv = delimited::parse("a;b\n1;2").unwrap();
    let rows = r#"<row><c t="inlineStr"><is><t>a</t></is></c><c t="inlineStr"><is><t>b</t></is></c></row><row><c><v>1</v></c><c><v>2</v></c></row>"#;
    assert_eq!(xlsx::read_grid(&workbook(rows, &[], false)).unwrap(), csv);
}
