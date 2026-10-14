use platform_values::Phone;

use super::DomainError;

const TELEGRAM_MIN: usize = 5;
const TELEGRAM_MAX: usize = 32;

/// Ник в Telegram в каноническом виде: без «@» и ссылки, в нижнем регистре.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Telegram(String);

impl Telegram {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let mut s = raw.trim().to_lowercase();
        for prefix in ["https://t.me/", "http://t.me/", "t.me/", "@"] {
            if let Some(rest) = s.strip_prefix(prefix) {
                s = rest.to_string();
                break;
            }
        }
        let valid = (TELEGRAM_MIN..=TELEGRAM_MAX).contains(&s.len())
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if valid {
            Ok(Self(s))
        } else {
            Err(DomainError::invalid(
                "telegram",
                "ник: 5-32 символа, латиница, цифры и подчёркивание",
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Номер WhatsApp - телефон, но отдельное поле: по рабочему номеру мессенджер часто молчит.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhatsApp(Phone);

impl WhatsApp {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        Phone::parse(raw)
            .map(Self)
            .map_err(|_| DomainError::invalid("whatsapp", "некорректный номер"))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
