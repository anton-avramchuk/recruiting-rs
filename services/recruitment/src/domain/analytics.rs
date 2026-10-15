//! Метрики найма: чистые функции над фактами об откликах (без БД и HTTP).
use std::collections::HashMap;

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use recruitment_contracts::Stage;

/// Этапы линии воронки по порядку; отказ в неё не входит.
pub const LINE: [Stage; 7] = [
    Stage::New,
    Stage::Screening,
    Stage::Hr,
    Stage::Tech,
    Stage::Final,
    Stage::Offer,
    Stage::Hired,
];

/// «Интервью HR» — порог, с которого отклик считается дошедшим до интервью.
const INTERVIEW_ORDINAL: usize = 3;

/// Название источника, когда он не указан.
pub const NO_SOURCE: &str = "Не указан";

/// Что известно об отклике для расчётов.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppFact {
    pub created_at: DateTime<Utc>,
    pub stage: Stage,
    pub stage_changed_at: DateTime<Utc>,
    pub source: Option<String>,
    /// Самый дальний этап линии по истории переходов (1…7), `0` — переходов нет.
    pub reached: usize,
    /// Время первого действия по отклику (кроме его создания).
    pub first_touch: Option<DateTime<Utc>>,
}

/// Номер этапа в линии воронки (1…7); у отказа `0`.
pub fn line_ordinal(stage: Stage) -> usize {
    LINE.iter().position(|s| *s == stage).map_or(0, |i| i + 1)
}

impl AppFact {
    /// До какого этапа линии дошёл отклик: максимум по истории и текущему этапу, не меньше первого.
    pub fn effective_reached(&self) -> usize {
        self.reached.max(line_ordinal(self.stage)).max(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunnelStep {
    pub stage: Stage,
    pub reached: i64,
    pub conversion: i64,
}

fn percent(part: i64, whole: i64) -> i64 {
    if whole <= 0 {
        0
    } else {
        (200 * part + whole) / (2 * whole)
    }
}

/// Воронка по достигнутым этапам и этап с наибольшей потерей.
pub fn funnel(facts: &[AppFact]) -> (Vec<FunnelStep>, Option<Stage>) {
    let reached_at = |ordinal: usize| {
        facts
            .iter()
            .filter(|f| f.effective_reached() >= ordinal)
            .count() as i64
    };
    let mut steps: Vec<FunnelStep> = Vec::with_capacity(LINE.len());
    for (i, stage) in LINE.iter().enumerate() {
        let reached = reached_at(i + 1);
        let conversion = match steps.last() {
            None => {
                if reached > 0 {
                    100
                } else {
                    0
                }
            }
            Some(prev) => percent(reached, prev.reached),
        };
        steps.push(FunnelStep {
            stage: *stage,
            reached,
            conversion,
        });
    }

    let mut worst: Option<(i64, Stage)> = None;
    for pair in steps.windows(2) {
        let (prev, step) = (&pair[0], &pair[1]);
        if prev.reached > 0
            && step.conversion < 100
            && worst.is_none_or(|(c, _)| step.conversion < c)
        {
            worst = Some((step.conversion, step.stage));
        }
    }
    (steps, worst.map(|(_, stage)| stage))
}

/// Медиана; для чётного числа значений — среднее двух средних.
pub fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) / 2.0
    })
}

/// Медиана срока найма в сутках по откликам, находящимся на «Выход».
pub fn days_to_hire(facts: &[AppFact]) -> Option<f64> {
    median(
        facts
            .iter()
            .filter(|f| f.stage == Stage::Hired)
            .map(|f| (f.stage_changed_at - f.created_at).num_seconds() as f64 / 86_400.0)
            .collect(),
    )
}

/// Медиана часов до первого действия по откликам, к которым прикасались.
pub fn first_touch_hours(facts: &[AppFact]) -> Option<f64> {
    median(
        facts
            .iter()
            .filter_map(|f| {
                f.first_touch
                    .map(|t| (t - f.created_at).num_seconds() as f64 / 3600.0)
            })
            .collect(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceStat {
    pub source: String,
    pub total: i64,
    pub interviews: i64,
    pub conversion: i64,
}

/// Источники: отклики и доля дошедших до интервью; по убыванию конверсии, затем числа откликов.
pub fn sources(facts: &[AppFact]) -> Vec<SourceStat> {
    let mut by_source: HashMap<String, (i64, i64)> = HashMap::new();
    for f in facts {
        let name = f
            .source
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(NO_SOURCE)
            .to_string();
        let entry = by_source.entry(name).or_default();
        entry.0 += 1;
        if f.effective_reached() >= INTERVIEW_ORDINAL {
            entry.1 += 1;
        }
    }
    let mut out: Vec<SourceStat> = by_source
        .into_iter()
        .map(|(source, (total, interviews))| SourceStat {
            source,
            total,
            interviews,
            conversion: percent(interviews, total),
        })
        .collect();
    out.sort_by(|a, b| {
        b.conversion
            .cmp(&a.conversion)
            .then(b.total.cmp(&a.total))
            .then(a.source.cmp(&b.source))
    });
    out
}

/// Понедельник недели, в которую попадает дата.
pub fn week_start(date: NaiveDate) -> NaiveDate {
    date - Duration::days(i64::from(date.weekday().num_days_from_monday()))
}

/// Последние `weeks` недель, заканчивая текущей, с нулями для недель без откликов.
pub fn week_series(
    counts: &[(NaiveDate, i64)],
    today: NaiveDate,
    weeks: usize,
) -> Vec<(NaiveDate, i64)> {
    let last = week_start(today);
    let first = last - Duration::weeks(weeks as i64 - 1);
    (0..weeks)
        .map(|i| {
            let week = first + Duration::weeks(i as i64);
            let count = counts
                .iter()
                .filter(|(d, _)| week_start(*d) == week)
                .map(|(_, n)| *n)
                .sum();
            (week, count)
        })
        .collect()
}
