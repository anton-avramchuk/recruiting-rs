//! Мастер импорта кандидатов: файл → проверка разбора → итог.
//!
//! Файл разбирается в браузере; на сервер уходят только проверенные строки пакетами.
use candidate_contracts::{ImportRequest, ImportRow};
use candidate_import::{
    columns::{self, Field},
    resume,
    rows::{self, education_text, jobs_text},
};
use leptos::prelude::*;
use leptos_router::components::A;
use recruitment_contracts::{BatchApplicationsRequest, Stage};

use ui_kit::{EmptyState, Notice, NoticeKind, PageHeader, SessionCtx};

use super::{
    import_file::{Loaded, current_year},
    list::can_write,
    server::import_persons,
};
use crate::funnel::server::{Reply, create_applications_batch, list_vacancies};

/// Строк в одном запросе импорта.
const CHUNK: usize = 200;
/// Идентификаторов в одном запросе пакетных откликов.
const APPLICATION_CHUNK: usize = 500;
const DEFAULT_SOURCE: &str = "Импорт из файла";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Pick,
    Table,
    Resume,
    Done,
}

/// Итог импорта для последнего шага.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Summary {
    created: i64,
    merged: i64,
    /// Номер строки в файле, поле, причина.
    rejected: Vec<(usize, String, String)>,
    applications_created: i64,
    applications_skipped: i64,
    /// Импорт остановился на ошибке; то, что успело пройти, учтено выше.
    error: Option<String>,
}

fn failure_text<T>(reply: Reply<T>) -> String {
    reply
        .failure()
        .unwrap_or_else(|| "Не удалось выполнить импорт.".into())
}

/// Отправляет строки пакетами, затем создаёт отклики на вакансию.
async fn run_import(
    rows: Vec<(usize, ImportRow)>,
    source: String,
    vacancy: String,
    stage: String,
    origin: String,
    progress: RwSignal<(usize, usize)>,
) -> Summary {
    let mut summary = Summary::default();
    let total = rows.len();
    let mut ids = Vec::new();
    progress.set((0, total));

    for (n, chunk) in rows.chunks(CHUNK).enumerate() {
        let request = ImportRequest {
            rows: chunk.iter().map(|(_, r)| r.clone()).collect(),
            default_source: Some(source.clone()),
        };
        match import_persons(request).await {
            Ok(Reply::Ok(response)) => {
                summary.created += response.created;
                summary.merged += response.merged;
                for result in response.results {
                    let file_row = chunk.get(result.row).map_or(0, |(r, _)| *r);
                    match (result.outcome.as_str(), result.id) {
                        ("rejected", _) => summary.rejected.push((
                            file_row,
                            result.field.unwrap_or_default(),
                            result.message.unwrap_or_default(),
                        )),
                        (_, Some(id)) => ids.push(id),
                        _ => {}
                    }
                }
                progress.set(((n * CHUNK + chunk.len()).min(total), total));
            }
            Ok(other) => {
                summary.error = Some(failure_text(other));
                return summary;
            }
            Err(_) => {
                summary.error = Some("Не удалось отправить строки: сервис недоступен.".into());
                return summary;
            }
        }
    }

    if let (false, Ok(vacancy_id)) = (vacancy.is_empty(), vacancy.parse()) {
        for chunk in ids.chunks(APPLICATION_CHUNK) {
            let request = BatchApplicationsRequest {
                vacancy_id,
                person_ids: chunk.to_vec(),
                stage: Some(stage.clone()).filter(|s| !s.is_empty()),
                origin: Some(origin.clone()),
            };
            match create_applications_batch(request).await {
                Ok(Reply::Ok(r)) => {
                    summary.applications_created += r.created;
                    summary.applications_skipped += r.skipped;
                }
                Ok(other) => {
                    summary.error = Some(format!(
                        "Кандидаты импортированы, но отклики не созданы: {}",
                        failure_text(other)
                    ));
                    return summary;
                }
                Err(_) => {
                    summary.error = Some(
                        "Кандидаты импортированы, но отклики не созданы: сервис недоступен.".into(),
                    );
                    return summary;
                }
            }
        }
    }
    summary
}

/// Поля формы резюме (правятся до импорта).
#[derive(Clone, Copy)]
struct ResumeForm {
    name: RwSignal<String>,
    phone: RwSignal<String>,
    email: RwSignal<String>,
    city: RwSignal<String>,
    salary: RwSignal<String>,
    experience: RwSignal<String>,
    skills: RwSignal<String>,
    jobs: RwSignal<String>,
    education: RwSignal<String>,
    about: RwSignal<String>,
}

impl ResumeForm {
    fn new() -> Self {
        Self {
            name: RwSignal::new(String::new()),
            phone: RwSignal::new(String::new()),
            email: RwSignal::new(String::new()),
            city: RwSignal::new(String::new()),
            salary: RwSignal::new(String::new()),
            experience: RwSignal::new(String::new()),
            skills: RwSignal::new(String::new()),
            jobs: RwSignal::new(String::new()),
            education: RwSignal::new(String::new()),
            about: RwSignal::new(String::new()),
        }
    }

    fn fill(&self, r: &resume::ParsedResume) {
        self.name.set(r.name.clone());
        self.phone.set(r.phone.clone());
        self.email.set(r.email.clone());
        self.city.set(r.city.clone());
        self.salary
            .set(r.salary.map(|s| s.to_string()).unwrap_or_default());
        self.experience.set(
            r.experience_years
                .map(|y| y.to_string())
                .unwrap_or_default(),
        );
        self.skills.set(r.skills.join(", "));
        self.jobs.set(jobs_text(&r.jobs));
        self.education.set(education_text(&r.education));
        self.about.set(r.about.clone());
    }

    /// Строка импорта из того, что стоит в форме.
    fn to_row(self, source: &str) -> ImportRow {
        let map = [
            Field::Name,
            Field::Phone,
            Field::Email,
            Field::City,
            Field::Salary,
            Field::Experience,
            Field::Skills,
            Field::Jobs,
            Field::Education,
            Field::About,
        ];
        let cells = [
            self.name.get_untracked(),
            self.phone.get_untracked(),
            self.email.get_untracked(),
            self.city.get_untracked(),
            self.salary.get_untracked(),
            self.experience.get_untracked(),
            self.skills.get_untracked(),
            self.jobs.get_untracked(),
            self.education.get_untracked(),
            self.about.get_untracked(),
        ];
        rows::build(&map, &cells, source)
    }
}

/// «найдено» либо «не найдено» рядом с подписью поля резюме.
fn found(value: RwSignal<String>) -> impl IntoView {
    view! {
        <span class=move || if value.get().trim().is_empty() { "miss" } else { "found" }>
            {move || if value.get().trim().is_empty() { "не найдено" } else { "найдено" }}
        </span>
    }
}

#[component]
pub fn ImportPage() -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let writable = Signal::derive(move || can_write(session));

    let step = RwSignal::new(Step::Pick);
    let error = RwSignal::new(None::<String>);
    let file_name = RwSignal::new(String::new());
    let grid = RwSignal::new(Vec::<Vec<String>>::new());
    let map = RwSignal::new(Vec::<Field>::new());
    let raw_text = RwSignal::new(String::new());
    let readable = RwSignal::new(true);
    let parsed = RwSignal::new(false);
    let form = ResumeForm::new();
    let vacancy = RwSignal::new(String::new());
    let stage = RwSignal::new("new".to_string());
    let source = RwSignal::new(DEFAULT_SOURCE.to_string());
    let busy = RwSignal::new(false);
    let progress = RwSignal::new((0usize, 0usize));
    let summary = RwSignal::new(Summary::default());
    let over = RwSignal::new(false);

    let vacancies = Resource::new(|| (), |_| list_vacancies());

    let reset = move || {
        step.set(Step::Pick);
        error.set(None);
        grid.set(Vec::new());
        map.set(Vec::new());
        raw_text.set(String::new());
        parsed.set(false);
        busy.set(false);
        summary.set(Summary::default());
    };

    let accept = move |loaded: Result<Loaded, String>| match loaded {
        Err(message) => error.set(Some(message)),
        Ok(Loaded::Table { name, grid: g }) => {
            error.set(None);
            file_name.set(name);
            map.set(columns::guess_all(&g[0]));
            grid.set(g);
            step.set(Step::Table);
        }
        Ok(Loaded::Resume {
            name,
            text,
            readable: ok,
        }) => {
            error.set(None);
            file_name.set(name);
            readable.set(ok);
            raw_text.set(text.clone());
            if ok {
                form.fill(&resume::parse(&text, current_year()));
                parsed.set(true);
            } else {
                parsed.set(false);
            }
            step.set(Step::Resume);
        }
    };

    #[cfg(feature = "hydrate")]
    let pick = move |file: Option<web_sys::File>| {
        let Some(file) = file else { return };
        leptos::task::spawn_local(async move {
            accept(super::import_file::read_file(file).await);
        });
    };
    #[cfg(not(feature = "hydrate"))]
    let pick = move |_: Option<()>| {
        let _ = accept;
    };

    let start = move || {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let (rows, resume_mode) = match step.get_untracked() {
            Step::Resume => (vec![(1, form.to_row(&source.get_untracked()))], true),
            _ => (
                rows::build_all(
                    &grid.get_untracked(),
                    &map.get_untracked(),
                    &source.get_untracked(),
                ),
                false,
            ),
        };
        let _ = resume_mode;
        let (src, vac, stg, origin) = (
            source.get_untracked(),
            vacancy.get_untracked(),
            stage.get_untracked(),
            file_name.get_untracked(),
        );
        leptos::task::spawn_local(async move {
            let result = run_import(rows, src, vac, stg, origin, progress).await;
            summary.set(result);
            busy.set(false);
            step.set(Step::Done);
        });
    };

    let destination = move || {
        view! {
            <div class="form-grid">
                <label>
                    "Вакансия"
                    <select data-testid="import-vacancy" on:change=move |ev| vacancy.set(event_target_value(&ev))>
                        <option value="">"— только в базу, без вакансии —"</option>
                        <Transition fallback=|| ()>
                            {move || {
                                vacancies
                                    .get()
                                    .and_then(Result::ok)
                                    .and_then(Reply::ok)
                                    .map(|list| {
                                        list.into_iter()
                                            .map(|v| view! { <option value=v.id.to_string()>{v.title}</option> })
                                            .collect_view()
                                    })
                            }}
                        </Transition>
                    </select>
                </label>
                <Show when=move || step.get() == Step::Resume>
                    <label>
                        "Этап"
                        <select data-testid="import-stage" on:change=move |ev| stage.set(event_target_value(&ev))>
                            {Stage::ALL
                                .into_iter()
                                .filter(|s| s.is_active() || *s == Stage::Hired)
                                .map(|s| view! { <option value=s.key()>{s.label()}</option> })
                                .collect_view()}
                        </select>
                    </label>
                </Show>
                <label>
                    "Источник"
                    <input
                        type="text"
                        data-testid="import-source"
                        prop:value=move || source.get()
                        on:input=move |ev| source.set(event_target_value(&ev))
                    />
                </label>
            </div>
        }
    };

    view! {
        <PageHeader title="Импорт кандидатов">
            <A href="/candidates" attr:class="btn">
                "К списку"
            </A>
        </PageHeader>
        <Show
            when=move || writable.get()
            fallback=|| {
                view! {
                    <div data-testid="import-forbidden">
                        <EmptyState
                            title="Импорт недоступен"
                            hint="Для импорта нужно право на изменение кандидатов."
                        />
                    </div>
                }
            }
        >
            <p class="steps muted">
                <b class:muted=move || step.get() != Step::Pick>"1. Файл"</b>
                " → "
                <b class:muted=move || !matches!(step.get(), Step::Table | Step::Resume)>
                    "2. Проверка разбора"
                </b>
                " → "
                <b class:muted=move || step.get() != Step::Done>"3. Готово"</b>
            </p>
            {move || error.get().map(|m| view! { <Notice kind=NoticeKind::Error><span data-testid="import-error">{m}</span></Notice> })}

            // шаг 1: файл
            <Show when=move || step.get() == Step::Pick>
                <div
                    class="drop"
                    class:drop--over=move || over.get()
                    data-testid="drop"
                    on:dragover=move |ev| {
                        ev.prevent_default();
                        over.set(true);
                    }
                    on:dragleave=move |_| over.set(false)
                    on:drop=move |ev| {
                        ev.prevent_default();
                        over.set(false);
                        #[cfg(feature = "hydrate")]
                        pick(ev.data_transfer().and_then(|d| d.files()).and_then(|f| f.get(0)));
                        #[cfg(not(feature = "hydrate"))]
                        {
                            let _ = &ev;
                            pick(None);
                        }
                    }
                >
                    <h2>"Перетащите файл сюда"</h2>
                    <p class="muted">"Колонки система распознает сама — настраивать ничего не нужно."</p>
                    <label class="btn btn--primary">
                        "Выбрать файл"
                        <input
                            type="file"
                            hidden
                            data-testid="file-input"
                            accept=".csv,.tsv,.txt,.xlsx,.pdf"
                            on:change=move |ev| {
                                #[cfg(feature = "hydrate")]
                                {
                                    let input = event_target::<web_sys::HtmlInputElement>(&ev);
                                    let file = input.files().and_then(|f| f.get(0));
                                    pick(file);
                                    input.set_value("");
                                }
                                #[cfg(not(feature = "hydrate"))]
                                {
                                    let _ = &ev;
                                    pick(None);
                                }
                            }
                        />
                    </label>
                    <div class="chips">
                        <span class="chip">".xlsx — выгрузка из Excel"</span>
                        <span class="chip">".csv / .tsv — таблица"</span>
                        <span class="chip">".pdf — резюме одного человека"</span>
                    </div>
                </div>
                <p class="muted">
                    "Файл не уходит на сервер: он разбирается в браузере, на сервер отправляются только проверенные строки."
                </p>
            </Show>

            // шаг 2: таблица
            <Show when=move || step.get() == Step::Table>
                <section class="card section" data-testid="import-table">
                    <h2>{move || format!("{} · строк: {}", file_name.get(), grid.with(|g| g.len().saturating_sub(1)))}</h2>
                    <div class="table-wrap">
                        <table class="maptbl">
                            <thead>
                                <tr>
                                    <th>"Колонка в файле"</th>
                                    <th>"Пример"</th>
                                    <th>"Что это"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {move || {
                                    let g = grid.get();
                                    g[0].iter()
                                        .enumerate()
                                        .map(|(i, header)| {
                                            let sample = g[1..]
                                                .iter()
                                                .find_map(|r| r.get(i).filter(|v| !v.is_empty()))
                                                .cloned()
                                                .unwrap_or_else(|| "—".into());
                                            let title = if header.is_empty() {
                                                format!("Колонка {}", i + 1)
                                            } else {
                                                header.clone()
                                            };
                                            view! {
                                                <tr data-testid="map-row">
                                                    <td><b>{title}</b></td>
                                                    <td class="muted">{sample}</td>
                                                    <td>
                                                        <select
                                                            data-col=i.to_string()
                                                            on:change=move |ev| {
                                                                let key = event_target_value(&ev);
                                                                map.update(|m| {
                                                                    if let Some(slot) = m.get_mut(i) {
                                                                        *slot = Field::from_key(&key).unwrap_or(Field::Skip);
                                                                    }
                                                                });
                                                            }
                                                        >
                                                            {Field::ALL
                                                                .into_iter()
                                                                .map(|f| {
                                                                    view! {
                                                                        <option
                                                                            value=f.key()
                                                                            selected=move || map.with(|m| m.get(i) == Some(&f))
                                                                        >
                                                                            {f.label()}
                                                                        </option>
                                                                    }
                                                                })
                                                                .collect_view()}
                                                        </select>
                                                    </td>
                                                </tr>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </tbody>
                        </table>
                    </div>
                    <h2>"Куда добавить"</h2>
                    {destination}
                    <p data-testid="import-hint">
                        {move || {
                            if map.with(|m| columns::has_name(m)) {
                                view! { <span class="found">"✓ имя найдено — можно импортировать"</span> }.into_any()
                            } else {
                                view! {
                                    <span class="miss">
                                        "Укажите, в какой колонке лежит имя, иначе кандидатов не создать."
                                    </span>
                                }
                                    .into_any()
                            }
                        }}
                    </p>
                    <p class="toolbar">
                        <button
                            class="btn btn--primary"
                            data-testid="import-start"
                            disabled=move || busy.get() || !map.with(|m| columns::has_name(m))
                            on:click=move |_| start()
                        >
                            {move || {
                                if busy.get() {
                                    let (done, total) = progress.get();
                                    format!("Импортируем… {done} из {total}")
                                } else {
                                    format!("Импортировать {}", grid.with(|g| g.len().saturating_sub(1)))
                                }
                            }}
                        </button>
                        <button class="btn" on:click=move |_| reset()>"Назад"</button>
                        <span class="muted">
                            "Совпадения по почте, телефону и имени дополнят существующих людей, дубли не создаются."
                        </span>
                    </p>
                </section>
            </Show>

            // шаг 2: резюме
            <Show when=move || step.get() == Step::Resume>
                <section class="card section" data-testid="import-resume">
                    <h2>{move || format!("Резюме · {}", file_name.get())}</h2>
                    <Show when=move || !parsed.get()>
                        <div data-testid="resume-paste">
                            <Notice kind=NoticeKind::Info>
                                {move || {
                                    if readable.get() {
                                        "Вставьте текст резюме, и система разберёт его."
                                    } else {
                                        "Из этого PDF не удалось прочитать текст (шрифт без таблицы Unicode). Скопируйте текст резюме и вставьте сюда."
                                    }
                                }}
                            </Notice>
                            <textarea
                                rows="8"
                                data-testid="resume-text"
                                placeholder="Вставьте текст резюме"
                                prop:value=move || raw_text.get()
                                on:input=move |ev| raw_text.set(event_target_value(&ev))
                            ></textarea>
                            <p>
                                <button
                                    class="btn btn--primary"
                                    data-testid="resume-parse"
                                    on:click=move |_| {
                                        let text = raw_text.get_untracked();
                                        if text.trim().is_empty() {
                                            error.set(Some("Вставьте текст резюме.".into()));
                                        } else {
                                            error.set(None);
                                            form.fill(&resume::parse(&text, current_year()));
                                            parsed.set(true);
                                        }
                                    }
                                >
                                    "Разобрать текст"
                                </button>
                                <button class="btn" on:click=move |_| reset()>"Другой файл"</button>
                            </p>
                        </div>
                    </Show>
                    <Show when=move || parsed.get()>
                        <div class="form-grid" data-testid="resume-form">
                            <label>
                                <span>"Имя и фамилия " {found(form.name)}</span>
                                <input type="text" data-testid="rf-name" prop:value=move || form.name.get() on:input=move |ev| form.name.set(event_target_value(&ev)) />
                            </label>
                            <label>
                                <span>"Телефон " {found(form.phone)}</span>
                                <input type="text" prop:value=move || form.phone.get() on:input=move |ev| form.phone.set(event_target_value(&ev)) />
                            </label>
                            <label>
                                <span>"Почта " {found(form.email)}</span>
                                <input type="text" prop:value=move || form.email.get() on:input=move |ev| form.email.set(event_target_value(&ev)) />
                            </label>
                            <label>
                                <span>"Город " {found(form.city)}</span>
                                <input type="text" data-testid="rf-city" prop:value=move || form.city.get() on:input=move |ev| form.city.set(event_target_value(&ev)) />
                            </label>
                            <label>
                                <span>"Ожидания, ₽ " {found(form.salary)}</span>
                                <input type="text" prop:value=move || form.salary.get() on:input=move |ev| form.salary.set(event_target_value(&ev)) />
                            </label>
                            <label>
                                <span>"Опыт, лет " {found(form.experience)}</span>
                                <input type="text" prop:value=move || form.experience.get() on:input=move |ev| form.experience.set(event_target_value(&ev)) />
                            </label>
                        </div>
                        <label>
                            <span>"Навыки через запятую " {found(form.skills)}</span>
                            <input type="text" prop:value=move || form.skills.get() on:input=move |ev| form.skills.set(event_target_value(&ev)) />
                        </label>
                        <label>
                            <span>"Опыт работы (период — компания — должность) " {found(form.jobs)}</span>
                            <textarea rows="4" prop:value=move || form.jobs.get() on:input=move |ev| form.jobs.set(event_target_value(&ev))></textarea>
                        </label>
                        <label>
                            <span>"Образование " {found(form.education)}</span>
                            <textarea rows="2" prop:value=move || form.education.get() on:input=move |ev| form.education.set(event_target_value(&ev))></textarea>
                        </label>
                        <label>
                            <span>"Кратко " {found(form.about)}</span>
                            <textarea rows="2" prop:value=move || form.about.get() on:input=move |ev| form.about.set(event_target_value(&ev))></textarea>
                        </label>
                        <h2>"Куда добавить"</h2>
                        {destination}
                        <p class="toolbar">
                            <button
                                class="btn btn--primary"
                                data-testid="import-start"
                                disabled=move || busy.get() || form.name.get().trim().is_empty()
                                on:click=move |_| start()
                            >
                                {move || if busy.get() { "Импортируем…" } else { "Добавить кандидата" }}
                            </button>
                            <button class="btn" on:click=move |_| reset()>"Другой файл"</button>
                        </p>
                    </Show>
                </section>
            </Show>

            // шаг 3: итог
            <Show when=move || step.get() == Step::Done>
                <section class="card section" data-testid="import-done">
                    <h2>"Импорт завершён"</h2>
                    <dl class="facts">
                        <dt>"Создано"</dt>
                        <dd data-testid="sum-created">{move || summary.get().created}</dd>
                        <dt>"Объединено с существующими"</dt>
                        <dd data-testid="sum-merged">{move || summary.get().merged}</dd>
                        <dt>"Отклонено"</dt>
                        <dd data-testid="sum-rejected">{move || summary.get().rejected.len()}</dd>
                        <Show when=move || !vacancy.get().is_empty()>
                            <dt>"Откликов создано"</dt>
                            <dd data-testid="sum-applications">{move || summary.get().applications_created}</dd>
                            <dt>"Откликов уже было"</dt>
                            <dd>{move || summary.get().applications_skipped}</dd>
                        </Show>
                    </dl>
                    {move || {
                        summary
                            .get()
                            .error
                            .map(|m| view! { <Notice kind=NoticeKind::Error><span data-testid="sum-error">{m}</span></Notice> })
                    }}
                    <Show when=move || !summary.get().rejected.is_empty()>
                        <h2>"Отклонённые строки"</h2>
                        <ul class="history" data-testid="rejected-list">
                            {move || {
                                summary
                                    .get()
                                    .rejected
                                    .into_iter()
                                    .map(|(row, field, message)| {
                                        view! {
                                            <li>
                                                <b>{format!("Строка {row}")}</b>
                                                {format!(" · {field}: {message}")}
                                            </li>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </ul>
                    </Show>
                    <p class="toolbar">
                        <A href="/candidates" attr:class="btn btn--primary" attr:data-testid="to-candidates">
                            "К списку кандидатов"
                        </A>
                        <A href="/applications" attr:class="btn">"К откликам"</A>
                        <button class="btn" on:click=move |_| reset()>"Импортировать ещё"</button>
                    </p>
                </section>
            </Show>
        </Show>
    }
}
