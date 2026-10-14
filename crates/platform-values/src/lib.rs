//! Значимые типы, общие для сервисов: нормализованные контакты.
//!
//! Значение проверяется и приводится к каноническому виду при создании, поэтому два написания
//! одного контакта равны, а негодное значение не доезжает до хранилища.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ValueError {
    #[error("invalid email")]
    InvalidEmail,
    #[error("invalid phone")]
    InvalidPhone,
}

/// Email: обрезанные пробелы, нижний регистр, проверка формы.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Email(String);

impl Email {
    pub fn parse(raw: &str) -> Result<Self, ValueError> {
        let s = raw.trim().to_lowercase();
        match s.split_once('@') {
            Some((local, domain))
                if !local.is_empty()
                    && domain.contains('.')
                    && !domain.starts_with('.')
                    && !domain.ends_with('.')
                    && !s.contains(char::is_whitespace) =>
            {
                Ok(Self(s))
            }
            _ => Err(ValueError::InvalidEmail),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Телефон: знак «+» и цифры, от 10 до 15 цифр. Российское написание с «8» приводится к «+7».
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Phone(String);

/// Сколько последних цифр считаются значимыми при сопоставлении людей.
const SIGNIFICANT_DIGITS: usize = 10;

impl Phone {
    pub fn parse(raw: &str) -> Result<Self, ValueError> {
        let s = raw.trim();
        let allowed = |(i, c): (usize, char)| {
            c.is_ascii_digit() || matches!(c, ' ' | '(' | ')' | '-' | '.') || (c == '+' && i == 0)
        };
        if !s.char_indices().all(|(i, c)| allowed((i, c))) {
            return Err(ValueError::InvalidPhone);
        }
        let mut digits: String = s.chars().filter(char::is_ascii_digit).collect();
        if !s.starts_with('+') && digits.len() == 11 && digits.starts_with('8') {
            digits.replace_range(0..1, "7");
        }
        if !(10..=15).contains(&digits.len()) {
            return Err(ValueError::InvalidPhone);
        }
        Ok(Self(format!("+{digits}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Последние десять цифр: ключ сопоставления, не зависящий от кода страны и написания.
    pub fn significant_digits(&self) -> &str {
        let digits = &self.0[1..];
        &digits[digits.len().saturating_sub(SIGNIFICANT_DIGITS)..]
    }
}
