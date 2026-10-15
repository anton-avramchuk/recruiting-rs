//! Разбор текста резюме в поля кандидата: порт эвристик прототипа без регулярных выражений.
//!
//! Эвристики намеренно простые: любое поле можно поправить до импорта, а ненайденное остаётся пустым.

/// Место работы: годы начала и окончания (`None` — по настоящее время).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub start_year: i32,
    pub end_year: Option<i32>,
    pub company: String,
    pub role: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Education {
    pub year: Option<i32>,
    pub institution: String,
    pub specialty: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedResume {
    pub name: String,
    pub phone: String,
    pub email: String,
    pub city: String,
    pub salary: Option<u64>,
    pub experience_years: Option<i32>,
    pub skills: Vec<String>,
    pub jobs: Vec<Job>,
    pub education: Vec<Education>,
    pub about: String,
}

const MAX_JOBS: usize = 6;
const MAX_EDUCATION: usize = 3;
const ABOUT_MIN_CHARS: usize = 55;
const ABOUT_MAX_CHARS: usize = 240;
/// Имя ищется среди первых строк резюме.
const NAME_SCAN_LINES: usize = 14;

/// Города: составные названия раньше простых, чтобы не принять часть за целое.
pub const CITIES: &[&str] = &[
    "Санкт-Петербург",
    "Нижний Новгород",
    "Ростов-на-Дону",
    "Москва",
    "Новосибирск",
    "Казань",
    "Екатеринбург",
    "Самара",
    "Воронеж",
    "Пермь",
    "Уфа",
    "Краснодар",
    "Омск",
    "Челябинск",
    "Волгоград",
    "Красноярск",
    "Тюмень",
    "Саратов",
    "Калининград",
    "Минск",
    "Ереван",
    "Ташкент",
    "Тбилиси",
    "Белград",
    "Алматы",
    "Астана",
];

/// Навыки: (как искать в нижнем регистре, как называть).
pub const SKILLS: &[(&str, &str)] = &[
    ("go", "Go"),
    ("golang", "Go"),
    ("rust", "Rust"),
    ("python", "Python"),
    ("java", "Java"),
    ("kotlin", "Kotlin"),
    ("swift", "Swift"),
    ("typescript", "TypeScript"),
    ("javascript", "JavaScript"),
    ("react", "React"),
    ("vue", "Vue"),
    ("angular", "Angular"),
    ("node", "Node"),
    ("php", "PHP"),
    ("c++", "C++"),
    ("c#", "C#"),
    (".net", ".NET"),
    ("sql", "SQL"),
    ("postgresql", "PostgreSQL"),
    ("mysql", "MySQL"),
    ("mongodb", "MongoDB"),
    ("redis", "Redis"),
    ("kafka", "Kafka"),
    ("rabbitmq", "RabbitMQ"),
    ("docker", "Docker"),
    ("kubernetes", "K8s"),
    ("k8s", "K8s"),
    ("git", "Git"),
    ("linux", "Linux"),
    ("grpc", "gRPC"),
    ("dbt", "dbt"),
    ("airflow", "Airflow"),
    ("spark", "Spark"),
    ("ml", "ML"),
    ("a/b", "A/B"),
    ("figma", "Figma"),
    ("excel", "Excel"),
    ("tableau", "Tableau"),
    ("1с", "1С"),
    ("1c", "1С"),
];

/// Служебные слова, по которым строка не может быть именем.
const NOT_A_NAME: &[&str] = &[
    "резюме",
    "curriculum",
    "vitae",
    "телефон",
    "почта",
    "e-mail",
    "email",
    "опыт",
    "навыки",
    "образован",
];

const EDUCATION_WORDS: &[&str] = &[
    "университет",
    "институт",
    "академ",
    "колледж",
    "university",
    "политех",
    "школа эконом",
];

fn lower(s: &str) -> Vec<char> {
    s.chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

/// Позиция первого вхождения `pat` как отдельного слова (границы — не буква и не цифра).
fn find_word(hay: &[char], pat: &[char]) -> Option<usize> {
    if pat.is_empty() || hay.len() < pat.len() {
        return None;
    }
    (0..=hay.len() - pat.len()).find(|&i| {
        hay[i..i + pat.len()] == *pat
            && (i == 0 || !is_word_char(hay[i - 1]))
            && hay.get(i + pat.len()).is_none_or(|c| !is_word_char(*c))
    })
}

fn find_sub(hay: &[char], pat: &[char]) -> Option<usize> {
    if pat.is_empty() || hay.len() < pat.len() {
        return None;
    }
    (0..=hay.len() - pat.len()).find(|&i| hay[i..i + pat.len()] == *pat)
}

fn contains(hay: &str, needle: &str) -> bool {
    find_sub(&lower(hay), &lower(needle)).is_some()
}

// ---------- контакты ----------

fn email(text: &[char]) -> String {
    let local = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-');
    let domain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-');
    for (at, c) in text.iter().enumerate() {
        if *c != '@' {
            continue;
        }
        let start = (0..at)
            .rev()
            .take_while(|&i| local(text[i]))
            .last()
            .unwrap_or(at);
        let end = (at + 1..text.len())
            .take_while(|&i| domain(text[i]))
            .last()
            .map_or(at, |i| i + 1);
        if start == at || end <= at + 1 {
            continue;
        }
        let host: String = text[at + 1..end].iter().collect();
        let host = host.trim_end_matches(['.', '-']);
        let tld = host.rsplit('.').next().unwrap_or("");
        if host.contains('.') && tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()) {
            let local: String = text[start..at].iter().collect();
            return format!("{local}@{host}");
        }
    }
    String::new()
}

fn phone(text: &[char]) -> String {
    let digit = |i: usize| text.get(i).is_some_and(char::is_ascii_digit);
    for start in 0..text.len() {
        // префикс: «+7» либо «8» на границе слова
        let mut at = if text[start] == '+' && text.get(start + 1) == Some(&'7') {
            start + 2
        } else if text[start] == '8' && (start == 0 || !is_word_char(text[start - 1])) {
            start + 1
        } else {
            continue;
        };
        let mut ok = true;
        for (count, separators) in [(3, " -("), (3, " -)"), (2, " -"), (2, " -")] {
            while text.get(at).is_some_and(|c| separators.contains(*c)) {
                at += 1;
            }
            if (0..count).all(|k| digit(at + k)) {
                at += count;
            } else {
                ok = false;
                break;
            }
        }
        if ok {
            return text[start..at].iter().collect();
        }
    }
    String::new()
}

// ---------- числа ----------

/// Число рядом со знаком рубля: «210000 руб», «250 000 ₽», «180 000 р.», «300000 RUB».
fn salary(text: &[char]) -> Option<u64> {
    let markers: [&[char]; 4] = [&['₽'], &['р', 'у', 'б'], &['р', '.'], &['r', 'u', 'b']];
    let low = text;
    for marker in markers {
        let mut from = 0;
        while let Some(rel) = find_sub(&low[from..], marker) {
            let at = from + rel;
            from = at + marker.len();
            let mut end = at;
            while end > 0 && low[end - 1].is_whitespace() {
                end -= 1;
            }
            let mut start = end;
            while start > 0 && (low[start - 1].is_ascii_digit() || low[start - 1].is_whitespace()) {
                start -= 1;
            }
            let digits: String = low[start..end]
                .iter()
                .filter(|c| c.is_ascii_digit())
                .collect();
            if digits.len() >= 4 {
                return digits.parse().ok();
            }
        }
    }
    None
}

/// Число из одной-двух цифр, за которым следует «год/года/лет», начиная с позиции `from`.
fn years_after(line: &[char], from: usize, limit: usize) -> Option<i32> {
    let end = (from + limit).min(line.len());
    let mut i = from;
    while i < end {
        if line[i].is_ascii_digit() {
            let start = i;
            while i < line.len() && line[i].is_ascii_digit() {
                i += 1;
            }
            if i - start <= 2 {
                let mut j = i;
                while j < line.len() && line[j].is_whitespace() && line[j] != '\n' {
                    j += 1;
                }
                let rest = &line[j..];
                let unit = |w: &str| rest.starts_with(&w.chars().collect::<Vec<_>>());
                if unit("год") || unit("лет") {
                    return line[start..i].iter().collect::<String>().parse().ok();
                }
            }
        } else {
            i += 1;
        }
    }
    None
}

fn experience(low: &[char]) -> Option<i32> {
    for line in low.split(|c| *c == '\n') {
        if let Some(at) = find_sub(line, &['о', 'п', 'ы', 'т'])
            && let Some(n) = years_after(line, at + 4, 30)
        {
            return Some(n);
        }
        // «5 лет опыта», «1 год опыт»
        let mut i = 0;
        while i < line.len() {
            if line[i].is_ascii_digit() {
                let start = i;
                while i < line.len() && line[i].is_ascii_digit() {
                    i += 1;
                }
                if i - start <= 2 {
                    let after: String = line[i..].iter().collect();
                    let after = after.trim_start();
                    let word_end = after
                        .strip_prefix("года")
                        .or_else(|| after.strip_prefix("год"))
                        .or_else(|| after.strip_prefix("лет"));
                    if let Some(rest) = word_end
                        && rest.trim_start().starts_with("опыт")
                    {
                        return line[start..i].iter().collect::<String>().parse().ok();
                    }
                }
            } else {
                i += 1;
            }
        }
    }
    None
}

// ---------- периоды ----------

struct Period {
    start: i32,
    end: Option<i32>,
    /// Границы совпадения в строке (в символах).
    range: (usize, usize),
}

fn year_at(line: &[char], at: usize) -> Option<i32> {
    let digits: String = line.get(at..at + 4)?.iter().collect();
    let before_ok = at == 0 || !line[at - 1].is_ascii_digit();
    let after_ok = line.get(at + 4).is_none_or(|c| !c.is_ascii_digit());
    if before_ok && after_ok && (digits.starts_with("19") || digits.starts_with("20")) {
        digits.parse().ok()
    } else {
        None
    }
}

/// Первый период «ГГГГ — ГГГГ» либо «ГГГГ — наст. время».
fn period(line: &[char]) -> Option<Period> {
    let low = lower(&line.iter().collect::<String>());
    for start in 0..line.len() {
        let Some(from) = year_at(line, start) else {
            continue;
        };
        let mut i = start + 4;
        while line.get(i).is_some_and(|c| c.is_whitespace()) {
            i += 1;
        }
        if line.get(i) == Some(&'г') {
            i += 1;
            if line.get(i) == Some(&'.') {
                i += 1;
            }
            while line.get(i).is_some_and(|c| c.is_whitespace()) {
                i += 1;
            }
        }
        let dashes = low[i.min(low.len())..]
            .iter()
            .take(2)
            .take_while(|c| matches!(c, '—' | '–' | '-'))
            .count();
        if dashes == 0 {
            continue;
        }
        i += dashes;
        while line.get(i).is_some_and(|c| c.is_whitespace()) {
            i += 1;
        }
        if let Some(to) = year_at(line, i) {
            return Some(Period {
                start: from,
                end: Some(to),
                range: (start, i + 4),
            });
        }
        let tail = &low[i.min(low.len())..];
        for word in [
            "по настоящее время",
            "по наст",
            "настоящее время",
            "наст",
            "present",
            "now",
            "сейчас",
        ] {
            let w: Vec<char> = word.chars().collect();
            if tail.starts_with(&w) {
                let mut end = i + w.len();
                // «наст. время», «наст.время»: добираем окончание
                if word == "наст" || word == "по наст" {
                    for extra in ["оящее", ".", " время", "время"] {
                        let e: Vec<char> = extra.chars().collect();
                        if low[end.min(low.len())..].starts_with(&e) {
                            end += e.len();
                        }
                    }
                    if low[end.min(low.len())..].starts_with(&[' ', 'в', 'р', 'е', 'м', 'я']) {
                        end += 6;
                    }
                }
                return Some(Period {
                    start: from,
                    end: None,
                    range: (start, end),
                });
            }
        }
    }
    None
}

fn cut(s: &str) -> String {
    s.trim_matches(|c: char| c.is_whitespace() || ",;:.—–-|".contains(c))
        .to_string()
}

/// Делит по запятой, вертикальной черте, длинному тире либо дефису с пробелами.
fn split_parts(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut parts = Vec::new();
    let mut current = String::new();
    for (i, c) in chars.iter().enumerate() {
        let spaced_hyphen = *c == '-'
            && i > 0
            && chars[i - 1].is_whitespace()
            && chars.get(i + 1).is_some_and(|n| n.is_whitespace());
        if matches!(c, ',' | '|' | '—' | '–') || spaced_hyphen {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(*c);
        }
    }
    parts.push(current);
    parts
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn years_in(line: &[char]) -> Vec<i32> {
    (0..line.len()).filter_map(|i| year_at(line, i)).collect()
}

fn remove_years(line: &[char]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < line.len() {
        if let Some(p) = period(&line[i..]).filter(|p| p.range.0 == 0) {
            i += p.range.1;
        } else if year_at(line, i).is_some() {
            i += 4;
        } else {
            out.push(line[i]);
            i += 1;
        }
    }
    out
}

fn name(lines: &[&str]) -> String {
    for line in lines.iter().take(NAME_SCAN_LINES) {
        if line.chars().count() > 60 || line.chars().any(|c| c.is_ascii_digit()) {
            continue;
        }
        if NOT_A_NAME.iter().any(|w| contains(line, w)) {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        let capitalized = |w: &str| {
            let mut chars = w.chars();
            let first = chars.next();
            let rest: Vec<char> = chars.collect();
            first.is_some_and(|c| c.is_uppercase() && (c.is_alphabetic()))
                && !rest.is_empty()
                && rest.iter().all(|c| c.is_lowercase() || *c == '-')
        };
        if (2..=3).contains(&words.len()) && words.iter().all(|w| capitalized(w)) {
            return words[..2].join(" ");
        }
    }
    String::new()
}

/// Разбирает текст резюме; `current_year` нужен, чтобы посчитать опыт по местам работы.
pub fn parse(text: &str, current_year: i32) -> ParsedResume {
    let text = text.replace('\r', "");
    let chars: Vec<char> = text.chars().collect();
    let low = lower(&text);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let mut r = ParsedResume {
        name: name(&lines),
        phone: phone(&chars),
        email: email(&chars),
        salary: salary(&low),
        experience_years: experience(&low),
        ..ParsedResume::default()
    };

    if let Some(city) = CITIES.iter().find(|c| find_sub(&low, &lower(c)).is_some()) {
        r.city = (*city).to_string();
    }

    // навыки в порядке появления в тексте, без повторов
    let mut found: Vec<(usize, &str)> = SKILLS
        .iter()
        .filter_map(|(pat, canonical)| {
            find_word(&low, &pat.chars().collect::<Vec<_>>()).map(|at| (at, *canonical))
        })
        .collect();
    found.sort();
    for (_, skill) in found {
        if !r.skills.iter().any(|s| s == skill) {
            r.skills.push(skill.to_string());
        }
    }

    // места работы и образование
    let mut used = vec![false; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        let line_chars: Vec<char> = line.chars().collect();
        if EDUCATION_WORDS.iter().any(|w| contains(line, w)) {
            if r.education.len() >= MAX_EDUCATION {
                continue;
            }
            used[i] = true;
            let rest = cut(&remove_years(&line_chars));
            let mut parts = split_parts(&rest).into_iter();
            let institution = parts.next().unwrap_or(rest.clone());
            r.education.push(Education {
                year: years_in(&line_chars).last().copied(),
                institution,
                specialty: parts.collect::<Vec<_>>().join(", "),
            });
            continue;
        }
        let Some(p) = period(&line_chars) else {
            continue;
        };
        if r.jobs.len() >= MAX_JOBS {
            continue;
        }
        used[i] = true;
        let mut rest = cut(&format!(
            "{}{}",
            line_chars[..p.range.0].iter().collect::<String>(),
            line_chars[p.range.1..].iter().collect::<String>()
        ));
        if rest.chars().count() < 3
            && let Some(next) = lines.get(i + 1)
            && period(&next.chars().collect::<Vec<_>>()).is_none()
        {
            rest = cut(next);
            used[i + 1] = true;
        }
        let mut parts = split_parts(&rest).into_iter();
        r.jobs.push(Job {
            start_year: p.start,
            end_year: p.end,
            company: parts.next().unwrap_or_default(),
            role: parts.next().unwrap_or_default(),
            note: parts.collect::<Vec<_>>().join(", "),
        });
    }

    if r.experience_years.is_none()
        && let Some(earliest) = r.jobs.iter().map(|j| j.start_year).min()
    {
        r.experience_years = Some((current_year - earliest).max(1));
    }

    if let Some(about) = lines.iter().enumerate().find_map(|(i, l)| {
        let l_chars: Vec<char> = l.chars().collect();
        (l.chars().count() > ABOUT_MIN_CHARS
            && !used[i]
            && period(&l_chars).is_none()
            && !NOT_A_NAME.iter().any(|w| contains(l, w))
            && !EDUCATION_WORDS.iter().any(|w| contains(l, w)))
        .then_some(*l)
    }) {
        r.about = about.chars().take(ABOUT_MAX_CHARS).collect();
    }
    r
}
