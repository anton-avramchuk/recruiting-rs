//! Реестр команд палитры: состав, поиск и группировка (без интерфейса).

/// Предел строк в палитре.
pub const MAX_ROWS: usize = 14;
/// Предел вакансий и людей в результатах поиска.
pub const MAX_VACANCIES: usize = 4;
pub const MAX_PEOPLE: usize = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Actions,
    Navigation,
    Vacancies,
    People,
    Settings,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Actions => "Действия",
            Group::Navigation => "Переход",
            Group::Vacancies => "Вакансии",
            Group::People => "Люди",
            Group::Settings => "Настройки",
        }
    }
}

/// Что делает выбранная строка.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Go(String),
    Cheatsheet,
    ToggleTheme,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub title: String,
    /// Подсказка справа: сочетание клавиш или пояснение.
    pub hint: String,
    pub group: Group,
    pub target: Target,
}

/// Найденная вакансия или человек.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub id: String,
    pub title: String,
    pub meta: String,
}

struct Command {
    title: &'static str,
    hint: &'static str,
    group: Group,
    target: Target,
    writes: bool,
}

fn command(
    title: &'static str,
    hint: &'static str,
    group: Group,
    target: Target,
    writes: bool,
) -> Command {
    Command {
        title,
        hint,
        group,
        target,
        writes,
    }
}

fn registry() -> Vec<Command> {
    let go = |p: &str| Target::Go(p.to_string());
    vec![
        command(
            "Разобрать отклики",
            "T",
            Group::Actions,
            go("/triage"),
            true,
        ),
        command(
            "Добавить кандидата",
            "",
            Group::Actions,
            go("/candidates/new"),
            true,
        ),
        command(
            "Создать вакансию",
            "",
            Group::Actions,
            go("/vacancies?new=1"),
            true,
        ),
        command(
            "Импорт из Excel, CSV или PDF",
            "I",
            Group::Actions,
            go("/candidates/import"),
            true,
        ),
        command("Сегодня", "G T", Group::Navigation, go("/"), false),
        command(
            "Вакансии и воронка",
            "G V",
            Group::Navigation,
            go("/vacancies"),
            false,
        ),
        command(
            "Отклики",
            "G O",
            Group::Navigation,
            go("/applications"),
            false,
        ),
        command(
            "База кандидатов",
            "G C",
            Group::Navigation,
            go("/candidates"),
            false,
        ),
        command(
            "Аналитика найма",
            "G A",
            Group::Navigation,
            go("/analytics"),
            false,
        ),
        command(
            "Переключить тему",
            "",
            Group::Settings,
            Target::ToggleTheme,
            false,
        ),
        command(
            "Горячие клавиши",
            "?",
            Group::Settings,
            Target::Cheatsheet,
            false,
        ),
    ]
}

/// Строки палитры для запроса. Команды фильтруются по вхождению без учёта регистра;
/// при непустом запросе добавляются найденные вакансии и люди.
pub fn results(query: &str, can_write: bool, vacancies: &[Hit], people: &[Hit]) -> Vec<Entry> {
    let q = query.trim().to_lowercase();
    let mut rows: Vec<Entry> = registry()
        .into_iter()
        .filter(|c| {
            (can_write || !c.writes) && (q.is_empty() || c.title.to_lowercase().contains(&q))
        })
        .map(|c| Entry {
            title: c.title.into(),
            hint: c.hint.into(),
            group: c.group,
            target: c.target,
        })
        .collect();
    if !q.is_empty() {
        rows.extend(vacancies.iter().take(MAX_VACANCIES).map(|h| Entry {
            title: h.title.clone(),
            hint: h.meta.clone(),
            group: Group::Vacancies,
            target: Target::Go("/vacancies".into()),
        }));
        rows.extend(people.iter().take(MAX_PEOPLE).map(|h| Entry {
            title: h.title.clone(),
            hint: h.meta.clone(),
            group: Group::People,
            target: Target::Go(format!("/candidates/{}", h.id)),
        }));
    }
    rows.truncate(MAX_ROWS);
    rows
}

/// Вакансии, подходящие под запрос по названию.
pub fn match_vacancies(query: &str, all: &[Hit]) -> Vec<Hit> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    all.iter()
        .filter(|h| h.title.to_lowercase().contains(&q))
        .take(MAX_VACANCIES)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str, title: &str) -> Hit {
        Hit {
            id: id.into(),
            title: title.into(),
            meta: "мета".into(),
        }
    }

    fn titles(rows: &[Entry]) -> Vec<&str> {
        rows.iter().map(|r| r.title.as_str()).collect()
    }

    #[test]
    fn an_empty_query_lists_actions_and_navigation_in_groups() {
        let rows = results("", true, &[], &[]);
        assert_eq!(rows[0].group, Group::Actions);
        assert!(titles(&rows).contains(&"Аналитика найма"));
        assert!(titles(&rows).contains(&"Горячие клавиши"));
        assert!(
            rows.iter()
                .all(|r| r.group != Group::People && r.group != Group::Vacancies)
        );
    }

    #[test]
    fn the_theme_command_is_in_settings_for_every_role() {
        for can_write in [true, false] {
            let rows = results("тему", can_write, &[], &[]);
            assert_eq!(titles(&rows), ["Переключить тему"]);
            assert_eq!(rows[0].group, Group::Settings);
            assert_eq!(rows[0].target, Target::ToggleTheme);
        }
    }

    #[test]
    fn the_filter_matches_a_substring_ignoring_case() {
        let rows = results("  АНАЛИТ ", true, &[], &[]);
        assert_eq!(titles(&rows), ["Аналитика найма"]);
        assert_eq!(rows[0].target, Target::Go("/analytics".into()));
    }

    #[test]
    fn a_viewer_gets_no_commands_that_change_data() {
        let rows = results("", false, &[], &[]);
        for banned in [
            "Разобрать отклики",
            "Добавить кандидата",
            "Создать вакансию",
            "Импорт из Excel, CSV или PDF",
        ] {
            assert!(!titles(&rows).contains(&banned), "{banned}");
        }
        assert!(titles(&rows).contains(&"Сегодня"));
        assert!(results("импорт", false, &[], &[]).is_empty());
    }

    #[test]
    fn found_vacancies_and_people_follow_commands_with_their_targets() {
        let rows = results(
            "а",
            true,
            &[hit("v1", "Аналитик")],
            &[hit("p1", "Анна Смирнова")],
        );
        let last_two: Vec<_> = rows.iter().rev().take(2).collect();
        assert_eq!(last_two[0].group, Group::People);
        assert_eq!(last_two[0].target, Target::Go("/candidates/p1".into()));
        assert_eq!(last_two[1].group, Group::Vacancies);
        assert_eq!(last_two[1].target, Target::Go("/vacancies".into()));
    }

    #[test]
    fn hits_are_ignored_without_a_query() {
        let rows = results("   ", true, &[hit("v", "Go")], &[hit("p", "Анна")]);
        assert!(
            rows.iter()
                .all(|r| r.group != Group::People && r.group != Group::Vacancies)
        );
    }

    #[test]
    fn limits_are_four_vacancies_seven_people_fourteen_rows() {
        let vacancies: Vec<_> = (0..9).map(|i| hit(&format!("v{i}"), "В")).collect();
        let people: Vec<_> = (0..20).map(|i| hit(&format!("p{i}"), "Л")).collect();
        let rows = results("zzz", true, &vacancies, &people);
        assert_eq!(
            rows.iter().filter(|r| r.group == Group::Vacancies).count(),
            4
        );
        assert_eq!(rows.iter().filter(|r| r.group == Group::People).count(), 7);
        assert!(results("", true, &vacancies, &people).len() <= MAX_ROWS);
        // «а» совпадает с несколькими командами, общий предел всё равно держится
        assert!(results("а", true, &vacancies, &people).len() <= MAX_ROWS);
    }

    #[test]
    fn the_total_is_capped_when_everything_matches() {
        let many: Vec<_> = (0..9).map(|i| hit(&i.to_string(), "х")).collect();
        let rows = results("о", true, &many, &many);
        assert_eq!(rows.len(), MAX_ROWS);
    }

    #[test]
    fn nothing_found_is_an_empty_list() {
        assert!(results("qqqq", true, &[], &[]).is_empty());
    }

    #[test]
    fn vacancy_matching_is_limited_and_needs_a_query() {
        let all: Vec<_> = (0..6)
            .map(|i| hit(&i.to_string(), &format!("Go-разработчик {i}")))
            .collect();
        assert_eq!(match_vacancies("go", &all).len(), 4);
        assert!(match_vacancies("", &all).is_empty());
        assert!(match_vacancies("rust", &all).is_empty());
    }
}
