use candidate_import::{ImportError, delimited};

fn sample() -> String {
    String::from_utf8(std::fs::read("tests/fixtures/demo-kandidaty.csv").unwrap()).unwrap()
}

#[test]
fn semicolon_sample_has_nine_columns_and_two_rows() {
    let grid = delimited::parse(&sample()).unwrap();
    assert_eq!(grid.len(), 3, "header + 2 rows");
    assert_eq!(grid[0].len(), 9);
    assert_eq!(grid[0][0], "Имя");
    assert_eq!(grid[0][8], "Образование");
    assert_eq!(grid[1][0], "Пётр Гусев");
    assert_eq!(grid[1][6], "Go, K8s, PostgreSQL", "quoted value with commas stays whole");
    assert_eq!(grid[1][7], "2021 — наст. время — Яндекс — Разработчик");
    assert_eq!(grid[2][2], "maria@mail.ru");
    assert!(grid.iter().all(|r| r.len() == 9));
}

#[test]
fn delimiter_is_detected_from_the_header() {
    let semi = delimited::parse("a;b;c\n1;2;3").unwrap();
    assert_eq!(semi[1], ["1", "2", "3"]);
    let comma = delimited::parse("a,b,c\n1,2,3").unwrap();
    assert_eq!(comma[1], ["1", "2", "3"]);
    let tab = delimited::parse("a\tb\tc\n1\t2\t3").unwrap();
    assert_eq!(tab[1], ["1", "2", "3"]);
}

#[test]
fn delimiters_inside_quotes_do_not_count() {
    // в заголовке запятые только внутри кавычек, настоящий разделитель - точка с запятой
    let grid = delimited::parse("\"a,b,c\";d\n1;2").unwrap();
    assert_eq!(grid[0], ["a,b,c", "d"]);
    assert_eq!(grid[1], ["1", "2"]);
}

#[test]
fn quotes_may_hold_escaped_quotes_and_line_breaks() {
    let grid = delimited::parse("a;b\n\"он сказал \"\"да\"\"\";\"строка 1\nстрока 2\"").unwrap();
    assert_eq!(grid[1][0], "он сказал \"да\"");
    assert_eq!(grid[1][1], "строка 1\nстрока 2");
}

#[test]
fn bom_crlf_and_blank_lines_are_ignored() {
    let grid = delimited::parse("\u{feff}a;b\r\n\r\n1;2\r\n   ;  \r\n3;4\r\n").unwrap();
    assert_eq!(grid.len(), 3);
    assert_eq!(grid[0], ["a", "b"]);
    assert_eq!(grid[2], ["3", "4"]);
}

#[test]
fn values_are_trimmed() {
    let grid = delimited::parse("a ; b\n 1 ;  2 ").unwrap();
    assert_eq!(grid[0], ["a", "b"]);
    assert_eq!(grid[1], ["1", "2"]);
}

#[test]
fn a_file_without_data_rows_is_an_error() {
    assert_eq!(delimited::parse("a;b;c"), Err(ImportError::NoData));
    assert_eq!(delimited::parse(""), Err(ImportError::NoData));
    assert_eq!(delimited::parse("\n\n  \n"), Err(ImportError::NoData));
    assert_eq!(ImportError::NoData.to_string(), "в файле нет строк с данными");
}

#[test]
fn short_rows_are_padded_to_the_header_width() {
    let grid = delimited::parse("a;b;c\n1;2").unwrap();
    assert_eq!(grid[1], ["1", "2", ""]);
}
