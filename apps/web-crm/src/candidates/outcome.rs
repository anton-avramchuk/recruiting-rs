//! Исходы обращений к сервису кандидатов в виде значений: интерфейс различает их без разбора HTTP.
use candidate_contracts::{DuplicateInfo, PersonDto, PersonPage};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LoadOutcome {
    Found(Box<PersonDto>),
    NotFound,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ListOutcome {
    Page(PersonPage),
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CreateOutcome {
    Created(Box<PersonDto>),
    /// Найден «тот же человек»; создание нужно подтвердить.
    Duplicate(DuplicateInfo),
    Invalid {
        field: String,
        message: String,
    },
    Forbidden,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SaveOutcome {
    Saved(Box<PersonDto>),
    Invalid { field: String, message: String },
    Forbidden,
    NotFound,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeleteOutcome {
    Deleted,
    Forbidden,
    NotFound,
    Unavailable,
}

fn invalid(body: &Value) -> (String, String) {
    (
        body["field"].as_str().unwrap_or_default().to_string(),
        body["detail"]
            .as_str()
            .unwrap_or("Проверьте введённые данные")
            .to_string(),
    )
}

fn person(body: &Value) -> Option<Box<PersonDto>> {
    serde_json::from_value(body.clone()).ok().map(Box::new)
}

pub fn classify_load(status: u16, body: &Value) -> LoadOutcome {
    match (status, person(body)) {
        (200, Some(p)) => LoadOutcome::Found(p),
        (404, _) => LoadOutcome::NotFound,
        _ => LoadOutcome::Unavailable,
    }
}

pub fn classify_list(status: u16, body: &Value) -> ListOutcome {
    match (status, serde_json::from_value(body.clone())) {
        (200, Ok(page)) => ListOutcome::Page(page),
        _ => ListOutcome::Unavailable,
    }
}

pub fn classify_create(status: u16, body: &Value) -> CreateOutcome {
    match status {
        201 => person(body).map_or(CreateOutcome::Unavailable, CreateOutcome::Created),
        409 => serde_json::from_value(body["duplicate"].clone())
            .map_or(CreateOutcome::Unavailable, CreateOutcome::Duplicate),
        400 => {
            let (field, message) = invalid(body);
            CreateOutcome::Invalid { field, message }
        }
        401 | 403 => CreateOutcome::Forbidden,
        _ => CreateOutcome::Unavailable,
    }
}

pub fn classify_save(status: u16, body: &Value) -> SaveOutcome {
    match status {
        200 => person(body).map_or(SaveOutcome::Unavailable, SaveOutcome::Saved),
        400 => {
            let (field, message) = invalid(body);
            SaveOutcome::Invalid { field, message }
        }
        401 | 403 => SaveOutcome::Forbidden,
        404 => SaveOutcome::NotFound,
        _ => SaveOutcome::Unavailable,
    }
}

pub fn classify_delete(status: u16) -> DeleteOutcome {
    match status {
        204 | 200 => DeleteOutcome::Deleted,
        401 | 403 => DeleteOutcome::Forbidden,
        404 => DeleteOutcome::NotFound,
        _ => DeleteOutcome::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn person_json() -> Value {
        json!({
            "id": "00000000-0000-0000-0000-000000000001", "full_name": "Иван", "source": null,
            "phone": null, "email": null, "telegram": null, "whatsapp": null,
            "expected_salary": null, "summary": null, "skills": [], "experience_years": 0,
            "employments": [], "educations": [], "created_at": "2026-01-01T00:00:00Z"
        })
    }

    #[test]
    fn load_distinguishes_found_missing_and_broken() {
        assert!(matches!(
            classify_load(200, &person_json()),
            LoadOutcome::Found(_)
        ));
        assert_eq!(classify_load(404, &Value::Null), LoadOutcome::NotFound);
        assert_eq!(classify_load(502, &Value::Null), LoadOutcome::Unavailable);
        assert_eq!(
            classify_load(200, &json!({"unexpected": true})),
            LoadOutcome::Unavailable
        );
    }

    #[test]
    fn create_reports_duplicates_with_the_matched_person() {
        let body = json!({"duplicate": {"id": "00000000-0000-0000-0000-000000000002", "full_name": "Иван", "matched_by": "email"}});
        match classify_create(409, &body) {
            CreateOutcome::Duplicate(d) => assert_eq!(d.matched_by, "email"),
            other => panic!("{other:?}"),
        }
        assert_eq!(classify_create(409, &json!({})), CreateOutcome::Unavailable);
    }

    #[test]
    fn invalid_input_carries_the_field_and_message() {
        let body = json!({"field": "email", "detail": "некорректный email"});
        assert_eq!(
            classify_create(400, &body),
            CreateOutcome::Invalid {
                field: "email".into(),
                message: "некорректный email".into()
            }
        );
        assert_eq!(
            classify_save(400, &json!({})),
            SaveOutcome::Invalid {
                field: String::new(),
                message: "Проверьте введённые данные".into()
            }
        );
    }

    #[test]
    fn auth_failures_map_to_forbidden_and_the_rest_to_unavailable() {
        for status in [401, 403] {
            assert_eq!(
                classify_create(status, &Value::Null),
                CreateOutcome::Forbidden
            );
            assert_eq!(classify_save(status, &Value::Null), SaveOutcome::Forbidden);
            assert_eq!(classify_delete(status), DeleteOutcome::Forbidden);
        }
        assert_eq!(classify_save(404, &Value::Null), SaveOutcome::NotFound);
        assert_eq!(classify_delete(404), DeleteOutcome::NotFound);
        assert_eq!(classify_delete(204), DeleteOutcome::Deleted);
        assert_eq!(classify_save(500, &Value::Null), SaveOutcome::Unavailable);
        assert_eq!(classify_list(500, &Value::Null), ListOutcome::Unavailable);
    }
}
