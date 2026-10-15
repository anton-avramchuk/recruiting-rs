//! Сопоставление колонок таблицы с полями кандидата по заголовкам.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// Колонка не используется.
    Skip,
    Name,
    Phone,
    Email,
    City,
    Role,
    Salary,
    Experience,
    Skills,
    Jobs,
    Education,
    Source,
    About,
}

impl Field {
    /// Все поля в порядке выбора в интерфейсе.
    pub const ALL: [Field; 13] = [
        Field::Skip,
        Field::Name,
        Field::Phone,
        Field::Email,
        Field::City,
        Field::Role,
        Field::Salary,
        Field::Experience,
        Field::Skills,
        Field::Jobs,
        Field::Education,
        Field::Source,
        Field::About,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Field::Skip => "",
            Field::Name => "name",
            Field::Phone => "phone",
            Field::Email => "email",
            Field::City => "city",
            Field::Role => "role",
            Field::Salary => "salary",
            Field::Experience => "exp",
            Field::Skills => "tags",
            Field::Jobs => "jobs",
            Field::Education => "edu",
            Field::Source => "src",
            Field::About => "about",
        }
    }

    pub fn from_key(key: &str) -> Option<Field> {
        Field::ALL.into_iter().find(|f| f.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Field::Skip => "— пропустить —",
            Field::Name => "Имя и фамилия",
            Field::Phone => "Телефон",
            Field::Email => "Почта",
            Field::City => "Город",
            Field::Role => "Должность",
            Field::Salary => "Ожидания, ₽",
            Field::Experience => "Опыт, лет",
            Field::Skills => "Навыки",
            Field::Jobs => "Опыт работы",
            Field::Education => "Образование",
            Field::Source => "Источник",
            Field::About => "Комментарий",
        }
    }

    /// Ключевые слова заголовка (в нижнем регистре).
    fn hints(self) -> &'static [&'static str] {
        match self {
            Field::Skip => &[],
            Field::Name => &["фио", "имя", "кандидат", "name", "сотрудник", "фамилия"],
            Field::Phone => &["тел", "phone", "моб", "контакт"],
            Field::Email => &["mail", "почт", "эл"],
            Field::City => &["город", "city", "регион", "локац"],
            Field::Role => &["должн", "позиц", "вакан", "role", "position", "профес"],
            Field::Salary => &["зарплат", "оклад", "ожид", "salary", "вилк", "доход"],
            Field::Experience => &["опыт", "стаж", "exp"],
            Field::Skills => &["навык", "skill", "стек", "технолог", "компетен"],
            Field::Jobs => &["компан", "места работы", "предыдущ", "работал", "employer"],
            Field::Education => &["образован", "вуз", "университет", "учеб"],
            Field::Source => &["источник", "source", "откуда", "канал"],
            Field::About => &["коммент", "замет", "note", "описан", "о себе"],
        }
    }
}

/// Поле по заголовку; если ничего не подошло, первая колонка считается именем.
pub fn guess(header: &str, index: usize) -> Field {
    let header = header.to_lowercase();
    Field::ALL
        .into_iter()
        .find(|f| f.hints().iter().any(|h| header.contains(h)))
        .unwrap_or(if index == 0 { Field::Name } else { Field::Skip })
}

pub fn guess_all(header: &[String]) -> Vec<Field> {
    header
        .iter()
        .enumerate()
        .map(|(i, h)| guess(h, i))
        .collect()
}

/// Есть ли колонка с именем: без неё кандидатов не создать.
pub fn has_name(map: &[Field]) -> bool {
    map.contains(&Field::Name)
}
