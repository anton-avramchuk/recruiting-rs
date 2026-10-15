//! Воронка найма: вакансии с доской, таблица откликов и панель человека.
pub mod applications;
pub mod format;
pub mod keys;
pub mod panel;
pub mod server;
pub mod today;
pub mod triage;
pub mod vacancies;

use leptos::prelude::*;

/// Состояние, общее для страницы и панели человека: какой отклик открыт и версия данных
/// (после любого изменения версия растёт, и всё, что от неё зависит, перечитывается).
#[derive(Clone, Copy)]
pub struct FunnelCtx {
    pub open: RwSignal<Option<String>>,
    pub version: RwSignal<u32>,
}

impl FunnelCtx {
    pub fn new() -> Self {
        Self {
            open: RwSignal::new(None),
            version: RwSignal::new(0),
        }
    }

    pub fn bump(&self) {
        self.version.update(|v| *v += 1);
    }
}

impl Default for FunnelCtx {
    fn default() -> Self {
        Self::new()
    }
}
