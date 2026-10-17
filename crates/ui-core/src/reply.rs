//! Единый исход обращения к серверу. Интерфейс различает его как значение, не разбирая HTTP.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

/// Исход обращения к серверу: успех либо причина, по которой действие не выполнено.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Reply<T> {
    Ok(T),
    Invalid {
        field: String,
        message: String,
    },
    Conflict(String),
    /// Конфликт с уже существующей записью; `existing` - её описание из ответа сервера.
    Duplicate {
        message: String,
        existing: Value,
    },
    Forbidden,
    NotFound,
    Unavailable,
}

impl<T> Reply<T> {
    pub fn ok(self) -> Option<T> {
        match self {
            Reply::Ok(v) => Some(v),
            _ => None,
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, Reply::Ok(_))
    }

    /// Сообщение для пользователя, когда действие не удалось; у успеха его нет.
    pub fn failure(&self) -> Option<String> {
        self.failure_with(|m| m.to_string())
    }

    /// То же, но текст конфликта проходит через `conflict`: так сообщения сервиса переводятся
    /// на язык интерфейса или заменяются фразой экрана.
    pub fn failure_with(&self, conflict: impl Fn(&str) -> String) -> Option<String> {
        self.failure_map(conflict, |m| m.to_string())
    }

    /// То же, но переводятся и текст конфликта, и текст ошибки проверки.
    pub fn failure_map(
        &self,
        conflict: impl Fn(&str) -> String,
        invalid: impl Fn(&str) -> String,
    ) -> Option<String> {
        Some(match self {
            Reply::Ok(_) => return None,
            Reply::Invalid { message, .. } => invalid(message),
            Reply::Conflict(m) | Reply::Duplicate { message: m, .. } => conflict(m),
            Reply::Forbidden => "Недостаточно прав для этого действия.".into(),
            Reply::NotFound => "Не найдено: возможно, запись уже удалена.".into(),
            Reply::Unavailable => "Сервис временно недоступен. Попробуйте позже.".into(),
        })
    }

    /// Преобразует данные успеха, остальные исходы сохраняются.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Reply<U> {
        match self {
            Reply::Ok(v) => Reply::Ok(f(v)),
            Reply::Invalid { field, message } => Reply::Invalid { field, message },
            Reply::Conflict(m) => Reply::Conflict(m),
            Reply::Duplicate { message, existing } => Reply::Duplicate { message, existing },
            Reply::Forbidden => Reply::Forbidden,
            Reply::NotFound => Reply::NotFound,
            Reply::Unavailable => Reply::Unavailable,
        }
    }
}

/// Преобразует ответ BFF в `Reply`; `parse` достаёт данные из успешного тела.
pub fn classify<T>(status: u16, body: &Value, parse: impl Fn(&Value) -> Option<T>) -> Reply<T> {
    let text = |key: &str| body[key].as_str().unwrap_or_default().to_string();
    match status {
        200 | 201 => parse(body).map_or(Reply::Unavailable, Reply::Ok),
        400 => Reply::Invalid {
            field: text("field"),
            message: match text("detail") {
                d if d.is_empty() => "Проверьте введённые данные".into(),
                d => d,
            },
        },
        401 | 403 => Reply::Forbidden,
        404 => Reply::NotFound,
        409 => {
            let message = match text("detail") {
                d if d.is_empty() => "Действие сейчас недоступно".to_string(),
                d => d,
            };
            match body.get("duplicate").filter(|d| !d.is_null()) {
                Some(existing) => Reply::Duplicate {
                    message,
                    existing: existing.clone(),
                },
                None => Reply::Conflict(message),
            }
        }
        _ => Reply::Unavailable,
    }
}

/// Ответ без тела: успех - 200 или 204.
pub fn classify_empty(status: u16, body: &Value) -> Reply<()> {
    match status {
        200 | 204 => Reply::Ok(()),
        other => classify(other, body, |_| Some(())),
    }
}

/// Разбирает тело успешного ответа в тип `T`.
pub fn typed<T: DeserializeOwned>(body: &Value) -> Option<T> {
    serde_json::from_value(body.clone()).ok()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_success_is_parsed_from_the_body() {
        assert_eq!(classify(200, &json!(7), |b| b.as_i64()), Reply::Ok(7));
        assert_eq!(classify(201, &json!(7), |b| b.as_i64()), Reply::Ok(7));
        assert_eq!(
            classify::<i64>(200, &json!("x"), |b| b.as_i64()),
            Reply::Unavailable
        );
    }

    #[test]
    fn a_validation_error_carries_the_field_and_the_message() {
        assert_eq!(
            classify::<u8>(400, &json!({"field": "email", "detail": "bad"}), |_| None),
            Reply::Invalid {
                field: "email".into(),
                message: "bad".into()
            }
        );
        let Reply::Invalid { message, .. } = classify::<u8>(400, &json!({}), |_| None) else {
            panic!("expected invalid");
        };
        assert_eq!(message, "Проверьте введённые данные");
    }

    #[test]
    fn statuses_map_to_replies() {
        assert_eq!(
            classify::<u8>(401, &json!(null), |_| None),
            Reply::Forbidden
        );
        assert_eq!(
            classify::<u8>(403, &json!(null), |_| None),
            Reply::Forbidden
        );
        assert_eq!(classify::<u8>(404, &json!(null), |_| None), Reply::NotFound);
        assert!(matches!(
            classify::<u8>(409, &json!({}), |_| None),
            Reply::Conflict(_)
        ));
        assert_eq!(
            classify::<u8>(502, &json!(null), |_| None),
            Reply::Unavailable
        );
        assert_eq!(
            classify::<u8>(500, &json!(null), |_| None),
            Reply::Unavailable
        );
    }

    #[test]
    fn an_empty_answer_succeeds_on_200_and_204() {
        assert_eq!(classify_empty(204, &json!(null)), Reply::Ok(()));
        assert_eq!(classify_empty(200, &json!(null)), Reply::Ok(()));
        assert_eq!(classify_empty(404, &json!(null)), Reply::NotFound);
        assert!(matches!(
            classify_empty(409, &json!({"detail": "x"})),
            Reply::Conflict(_)
        ));
        assert_eq!(classify_empty(502, &json!(null)), Reply::Unavailable);
    }

    #[test]
    fn a_failure_message_exists_only_for_failures() {
        assert!(Reply::Ok(1).failure().is_none());
        assert!(Reply::<u8>::Forbidden.failure().is_some());
        assert_eq!(
            Reply::<u8>::Conflict("нельзя".into()).failure().as_deref(),
            Some("нельзя")
        );
    }

    #[test]
    fn a_conflict_can_be_translated_by_the_screen() {
        let reply: Reply<()> = Reply::Conflict("email already registered".into());
        assert_eq!(
            reply
                .failure_with(|m| {
                    if m.contains("email") {
                        "Такой email уже есть.".into()
                    } else {
                        m.to_string()
                    }
                })
                .as_deref(),
            Some("Такой email уже есть.")
        );
        // остальные исходы переводу не подвергаются
        assert_eq!(
            Reply::<()>::NotFound.failure_with(|_| "x".into()),
            Reply::<()>::NotFound.failure()
        );
        assert_eq!(Reply::Ok(1).failure_with(|_| "x".into()), None);
    }

    #[test]
    fn a_validation_message_can_be_translated_too() {
        let reply: Reply<()> = Reply::Invalid {
            field: "name".into(),
            message: "too long".into(),
        };
        assert_eq!(
            reply
                .failure_map(|m| m.to_string(), |_| "Слишком длинно.".into())
                .as_deref(),
            Some("Слишком длинно.")
        );
        // failure_with ошибку проверки не меняет
        assert_eq!(
            reply.failure_with(|_| "x".into()).as_deref(),
            Some("too long")
        );
    }

    #[test]
    fn a_conflict_with_an_existing_record_carries_it_as_data() {
        let body =
            json!({"detail": "possible duplicate", "duplicate": {"id": "p1", "full_name": "Анна"}});
        let reply = classify::<u8>(409, &body, |_| None);
        assert_eq!(
            reply,
            Reply::Duplicate {
                message: "possible duplicate".into(),
                existing: json!({"id": "p1", "full_name": "Анна"})
            }
        );
        // для пользователя это конфликт: текст проходит через перевод
        assert_eq!(
            reply.failure_with(|_| "Такой уже есть.".into()).as_deref(),
            Some("Такой уже есть.")
        );
        assert!(!reply.is_ok());
        // без данных о записи - обычный конфликт
        assert!(matches!(
            classify::<u8>(409, &json!({"detail": "x"}), |_| None),
            Reply::Conflict(_)
        ));
    }

    #[test]
    fn map_changes_only_the_success() {
        assert_eq!(Reply::Ok(2).map(|n| n * 2), Reply::Ok(4));
        assert_eq!(Reply::<i32>::NotFound.map(|n| n * 2), Reply::NotFound);
        assert_eq!(
            Reply::<i32>::Invalid {
                field: "f".into(),
                message: "m".into()
            }
            .map(|n| n + 1),
            Reply::Invalid {
                field: "f".into(),
                message: "m".into()
            }
        );
    }

    #[test]
    fn typed_parses_or_gives_nothing() {
        assert_eq!(typed::<Vec<u8>>(&json!([1, 2])), Some(vec![1, 2]));
        assert_eq!(typed::<Vec<u8>>(&json!("x")), None);
    }
}
