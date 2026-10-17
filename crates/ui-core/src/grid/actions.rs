//! Действия строки грида: какие показывать и когда двойной щелчок открывает запись.

/// Действия, доступные в строке.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowActions {
    pub edit: bool,
    pub remove: bool,
}

impl RowActions {
    /// «Изменить» есть, только если у строки есть ссылка на запись; «Удалить» - если задан обработчик
    /// удаления и он разрешает удалять эту запись.
    pub fn new(has_href: bool, has_remove: bool, remove_allowed: bool) -> Self {
        Self {
            edit: has_href,
            remove: has_remove && remove_allowed,
        }
    }

    /// Нужен ли столбец действий хотя бы с одним действием.
    pub fn any(self) -> bool {
        self.edit || self.remove
    }
}

/// Элемент внутри строки, у которого есть собственное поведение: двойной щелчок по нему
/// не должен открывать запись.
pub fn is_interactive(tag: &str) -> bool {
    matches!(
        tag.to_ascii_lowercase().as_str(),
        "a" | "button" | "input" | "select" | "textarea" | "label" | "option"
    )
}

/// Клавиша, закрывающая диалог подтверждения.
pub fn closes_dialog(key: &str) -> bool {
    key == "Escape"
}

/// Вопрос диалога подтверждения удаления.
pub fn confirm_question(label: &str) -> String {
    let label = label.trim();
    if label.is_empty() {
        "Удалить запись? Действие необратимо.".into()
    } else {
        format!("Удалить «{label}»? Действие необратимо.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_needs_a_link_and_remove_needs_a_handler_that_allows_it() {
        assert_eq!(
            RowActions::new(true, false, true),
            RowActions {
                edit: true,
                remove: false
            }
        );
        assert_eq!(
            RowActions::new(true, true, true),
            RowActions {
                edit: true,
                remove: true
            }
        );
        assert_eq!(
            RowActions::new(true, true, false),
            RowActions {
                edit: true,
                remove: false
            }
        );
        assert_eq!(
            RowActions::new(false, true, true),
            RowActions {
                edit: false,
                remove: true
            }
        );
    }

    #[test]
    fn a_row_without_any_action_has_no_actions_column() {
        assert!(!RowActions::new(false, false, true).any());
        assert!(!RowActions::new(false, true, false).any());
        assert!(RowActions::new(true, false, false).any());
    }

    #[test]
    fn a_double_click_on_inner_controls_does_not_open_the_row() {
        for tag in ["A", "button", "INPUT", "select", "textarea", "label"] {
            assert!(is_interactive(tag), "{tag}");
        }
        for tag in ["td", "span", "tr", "div"] {
            assert!(!is_interactive(tag), "{tag}");
        }
    }

    #[test]
    fn only_escape_closes_the_dialog() {
        assert!(closes_dialog("Escape"));
        assert!(!closes_dialog("Enter"));
        assert!(!closes_dialog("a"));
    }

    #[test]
    fn the_question_names_the_record() {
        assert_eq!(
            confirm_question(" Стажёр "),
            "Удалить «Стажёр»? Действие необратимо."
        );
        assert_eq!(
            confirm_question("  "),
            "Удалить запись? Действие необратимо."
        );
    }
}
