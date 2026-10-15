use chrono::{DateTime, FixedOffset, Utc};
use platform_core::tenant::TenantId;
use recruitment_contracts::{HR_SYSTEMS, Stage};
use uuid::Uuid;

use super::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Created,
    Stage,
    Rejected,
    Letter,
    Interview,
    Handoff,
}

impl EventKind {
    pub fn key(self) -> &'static str {
        match self {
            EventKind::Created => "created",
            EventKind::Stage => "stage",
            EventKind::Rejected => "rejected",
            EventKind::Letter => "letter",
            EventKind::Interview => "interview",
            EventKind::Handoff => "handoff",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        [
            EventKind::Created,
            EventKind::Stage,
            EventKind::Rejected,
            EventKind::Letter,
            EventKind::Interview,
            EventKind::Handoff,
        ]
        .into_iter()
        .find(|k| k.key() == key)
    }
}

/// Запись истории, ещё не сохранённая.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEvent {
    pub kind: EventKind,
    pub text: String,
    pub at: DateTime<Utc>,
}

/// Агрегат «отклик»: связь человека с вакансией и положение в воронке.
/// Изменения копят записи истории, которые хранилище забирает через `take_events`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Application {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub person_id: Uuid,
    pub vacancy_id: Uuid,
    pub stage: Stage,
    pub stage_changed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    events: Vec<NewEvent>,
}

impl Application {
    pub fn create(
        tenant_id: TenantId,
        person_id: Uuid,
        vacancy_id: Uuid,
        now: DateTime<Utc>,
    ) -> Self {
        let mut app = Self {
            id: Uuid::new_v4(),
            tenant_id,
            person_id,
            vacancy_id,
            stage: Stage::New,
            stage_changed_at: now,
            created_at: now,
            events: Vec::new(),
        };
        app.record(EventKind::Created, "Получен отклик".into(), now);
        app
    }

    /// Восстановление из хранилища.
    pub fn restore(
        id: Uuid,
        tenant_id: TenantId,
        person_id: Uuid,
        vacancy_id: Uuid,
        stage: Stage,
        stage_changed_at: DateTime<Utc>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            tenant_id,
            person_id,
            vacancy_id,
            stage,
            stage_changed_at,
            created_at,
            events: Vec::new(),
        }
    }

    fn record(&mut self, kind: EventKind, text: String, at: DateTime<Utc>) {
        self.events.push(NewEvent { kind, text, at });
    }

    /// Накопленные записи истории (забираются один раз).
    pub fn take_events(&mut self) -> Vec<NewEvent> {
        std::mem::take(&mut self.events)
    }

    /// Переводит на этап; `false` - отклик уже на нём и ничего не изменилось.
    pub fn move_to(&mut self, stage: Stage, now: DateTime<Utc>) -> bool {
        if self.stage == stage {
            return false;
        }
        self.stage = stage;
        self.stage_changed_at = now;
        self.record(EventKind::Stage, format!("Этап → {}", stage.label()), now);
        true
    }

    /// Следующий этап по порядку; с «Выход» и «Отказ» перейти некуда.
    pub fn advance(&mut self, now: DateTime<Utc>) -> Result<(), DomainError> {
        let next = self.stage.next().ok_or_else(|| {
            DomainError::conflict(format!("с этапа «{}» дальше некуда", self.stage.label()))
        })?;
        self.move_to(next, now);
        Ok(())
    }

    /// Отказ с необязательной причиной; письмо с отказом имитируется записью.
    pub fn reject(&mut self, reason: Option<String>, now: DateTime<Utc>) {
        let reason = reason
            .map(|r| r.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|r| !r.is_empty());
        if self.stage != Stage::Rejected {
            self.stage = Stage::Rejected;
            self.stage_changed_at = now;
        }
        let text = match reason {
            Some(r) => format!("Отказ: {r}"),
            None => "Отказ".into(),
        };
        self.record(EventKind::Rejected, text, now);
        self.record(EventKind::Letter, "Письмо с отказом отправлено".into(), now);
    }

    /// Назначает интервью; время показывается в часовом поясе пользователя
    /// (смещение от UTC в секундах), приглашение имитируется записью.
    pub fn schedule_interview(
        &mut self,
        at: DateTime<Utc>,
        offset_seconds: i32,
        now: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        if at <= now {
            return Err(DomainError::invalid("at", "время интервью уже прошло"));
        }
        let offset = FixedOffset::east_opt(offset_seconds)
            .ok_or_else(|| DomainError::invalid("at", "некорректный часовой пояс"))?;
        let local = at.with_timezone(&offset);
        self.record(
            EventKind::Interview,
            format!("Интервью назначено на {}", local.format("%d.%m.%Y %H:%M")),
            now,
        );
        self.record(
            EventKind::Letter,
            "Приглашение на интервью отправлено".into(),
            now,
        );
        Ok(())
    }

    /// Передача вышедшего кандидата в кадровую систему (имитация записью).
    pub fn hand_off(&mut self, system: &str, now: DateTime<Utc>) -> Result<(), DomainError> {
        if self.stage != Stage::Hired {
            return Err(DomainError::conflict(
                "передать можно только вышедшего кандидата",
            ));
        }
        let system = HR_SYSTEMS
            .iter()
            .find(|s| s.eq_ignore_ascii_case(system.trim()))
            .ok_or_else(|| DomainError::invalid("system", "неизвестная кадровая система"))?;
        self.record(
            EventKind::Handoff,
            format!("Карточка передана в {system}"),
            now,
        );
        Ok(())
    }

    /// Письмо кандидату (имитация записью в историю).
    pub fn send_letter(&mut self, now: DateTime<Utc>) {
        self.record(EventKind::Letter, "Письмо кандидату отправлено".into(), now);
    }
}
