//! Блок «Комментарии»: лента, добавление, правка и удаление на месте.
use comment_contracts::CommentDto;
use leptos::{prelude::*, task::spawn_local};
use ui_kit::{Notice, NoticeKind, Reply, SessionCtx};

use super::{
    reload_limit, rights,
    server::{create_comment, delete_comment, edit_comment, list_comments},
};
use crate::{candidates::list::can_write, funnel::format::short_moment};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Load {
    Loading,
    Ready,
    Failed,
}

/// Лента комментариев объекта `target_type` (`client`, `vacancy`, `person`) с идентификатором
/// `target_id`. При смене идентификатора лента загружается заново.
#[component]
pub fn Comments(
    target_type: &'static str,
    #[prop(into)] target_id: Signal<String>,
) -> impl IntoView {
    let session = expect_context::<SessionCtx>();
    let writable = Signal::derive(move || can_write(session));
    let identity = move || {
        session
            .current()
            .and_then(Result::ok)
            .and_then(|s| s.user)
            .map(|u| {
                let remove_any = crate::access::can_delete(&u);
                (u.id, remove_any)
            })
            .unwrap_or_default()
    };

    let items = RwSignal::new(Vec::<CommentDto>::new());
    let next = RwSignal::new(None::<String>);
    let load = RwSignal::new(Load::Loading);
    let error = RwSignal::new(None::<String>);
    let editing = RwSignal::new(None::<String>);
    let draft = RwSignal::new(String::new());
    let edit_text = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    // первая страница (или столько, сколько уже показано, после действия)
    let fetch = move |limit: i64| {
        let id = target_id.get_untracked();
        spawn_local(async move {
            match list_comments(target_type.to_string(), id.clone(), None, limit).await {
                Ok(Reply::Ok(page)) => {
                    // ответ на устаревшую цель (экран уже сменился) отбрасывается
                    if target_id.try_get_untracked().as_deref() == Some(id.as_str()) {
                        items.try_set(page.items);
                        next.try_set(page.next);
                        load.try_set(Load::Ready);
                    }
                }
                _ => {
                    load.try_set(Load::Failed);
                }
            }
        });
    };
    Effect::new(move |_| {
        target_id.track();
        load.set(Load::Loading);
        fetch(50);
    });

    let more = move |_| {
        let Some(cursor) = next.get_untracked() else {
            return;
        };
        let id = target_id.get_untracked();
        spawn_local(async move {
            match list_comments(target_type.to_string(), id, Some(cursor), 50).await {
                Ok(Reply::Ok(page)) => {
                    items.try_update(|list| list.extend(page.items));
                    next.try_set(page.next);
                }
                _ => {
                    error.try_set(Some("Не удалось загрузить продолжение.".into()));
                }
            }
        });
    };

    let reload = move || fetch(reload_limit(items.get_untracked().len()));

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let text = draft.get_untracked();
        if text.trim().is_empty() || busy.get_untracked() {
            return;
        }
        busy.set(true);
        let id = target_id.get_untracked();
        spawn_local(async move {
            match create_comment(target_type.to_string(), id, text).await {
                Ok(Reply::Ok(_)) => {
                    error.try_set(None);
                    draft.try_set(String::new());
                    reload();
                }
                Ok(other) => {
                    error.try_set(other.failure());
                }
                Err(_) => {
                    error.try_set(Some("Не удалось отправить комментарий.".into()));
                }
            }
            busy.try_set(false);
        });
    };

    let save_edit = move |id: String| {
        let text = edit_text.get_untracked();
        if text.trim().is_empty() || busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match edit_comment(id, text).await {
                Ok(Reply::Ok(_)) => {
                    error.try_set(None);
                    editing.try_set(None);
                    reload();
                }
                Ok(other) => {
                    error.try_set(other.failure());
                }
                Err(_) => {
                    error.try_set(Some("Не удалось сохранить правку.".into()));
                }
            }
            busy.try_set(false);
        });
    };

    let remove = move |id: String| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match delete_comment(id).await {
                Ok(Reply::Ok(())) | Ok(Reply::NotFound) => {
                    error.try_set(None);
                    reload();
                }
                Ok(other) => {
                    error.try_set(other.failure());
                }
                Err(_) => {
                    error.try_set(Some("Не удалось удалить комментарий.".into()));
                }
            }
            busy.try_set(false);
        });
    };

    view! {
        <section class="card section comments" data-testid="comments">
            <div class="section__head">
                <h2>"Комментарии"</h2>
            </div>
            {move || error.get().map(|e| view! { <div data-testid="comments-error"><Notice kind=NoticeKind::Error>{e}</Notice></div> })}
            {move || match load.get() {
                Load::Loading => view! { <p class="muted">"Загрузка…"</p> }.into_any(),
                Load::Failed => {
                    view! {
                        <div data-testid="comments-failed">
                            <Notice kind=NoticeKind::Error>"Не удалось загрузить комментарии."</Notice>
                            <button
                                class="btn"
                                data-testid="comments-retry"
                                on:click=move |_| {
                                    load.set(Load::Loading);
                                    fetch(50);
                                }
                            >
                                "Повторить"
                            </button>
                        </div>
                    }
                        .into_any()
                }
                Load::Ready if items.get().is_empty() => {
                    view! { <p class="muted" data-testid="comments-empty">"Комментариев пока нет."</p> }.into_any()
                }
                Load::Ready => {
                    view! {
                        <div class="comments__list" data-testid="comment-list">
                            <For
                                each=move || items.get()
                                key=|c| (c.id, c.edited_at.clone())
                                children=move |c| {
                                    let (me, is_owner) = identity();
                                    let r = rights(writable.get_untracked(), is_owner, &me, &c.author_id.to_string());
                                    let id = c.id.to_string();
                                    let edit_id = id.clone();
                                    let delete_id = id.clone();
                                    let save_id = id.clone();
                                    let body = c.body.clone();
                                    let start_body = c.body.clone();
                                    let is_editing = {
                                        let id = id.clone();
                                        move || editing.get().as_deref() == Some(id.as_str())
                                    };
                                    view! {
                                        <article class="comment" data-testid="comment">
                                            <header class="comment__head">
                                                <b data-testid="comment-author">{c.author_name.clone()}</b>
                                                <time class="muted">{short_moment(&c.created_at)}</time>
                                                {c.edited_at.as_ref().map(|_| view! { <span class="chip" data-testid="comment-edited">"изменено"</span> })}
                                                <span class="spacer"></span>
                                                {r.edit.then(|| {
                                                    view! {
                                                        <button
                                                            class="btn btn--ghost"
                                                            data-testid="comment-edit"
                                                            on:click={
                                                                let edit_id = edit_id.clone();
                                                                let start_body = start_body.clone();
                                                                move |_| {
                                                                    edit_text.set(start_body.clone());
                                                                    editing.set(Some(edit_id.clone()));
                                                                }
                                                            }
                                                        >
                                                            "Изменить"
                                                        </button>
                                                    }
                                                })}
                                                {r.delete.then(|| {
                                                    view! {
                                                        <button
                                                            class="btn btn--ghost"
                                                            data-testid="comment-delete"
                                                            on:click={
                                                                let delete_id = delete_id.clone();
                                                                move |_| remove(delete_id.clone())
                                                            }
                                                        >
                                                            "Удалить"
                                                        </button>
                                                    }
                                                })}
                                            </header>
                                            <Show
                                                when=is_editing
                                                fallback=move || view! { <p class="comment__body" data-testid="comment-body">{body.clone()}</p> }
                                            >
                                                <form
                                                    method="post"
                                                    onsubmit="return false"
                                                    on:submit={
                                                        let save_id = save_id.clone();
                                                        move |ev| {
                                                            ev.prevent_default();
                                                            save_edit(save_id.clone());
                                                        }
                                                    }
                                                >
                                                    <textarea
                                                        rows="3"
                                                        aria-label="Текст комментария"
                                                        data-testid="comment-edit-text"
                                                        prop:value=move || edit_text.get()
                                                        on:input=move |ev| edit_text.set(event_target_value(&ev))
                                                    ></textarea>
                                                    <p>
                                                        <button type="submit" class="btn btn--primary" data-testid="comment-save" disabled=move || busy.get()>
                                                            "Сохранить"
                                                        </button>
                                                        <button type="button" class="btn btn--ghost" data-testid="comment-cancel" on:click=move |_| editing.set(None)>
                                                            "Отмена"
                                                        </button>
                                                    </p>
                                                </form>
                                            </Show>
                                        </article>
                                    }
                                }
                            />
                        </div>
                        {move || {
                            next.get()
                                .map(|_| {
                                    view! {
                                        <button class="btn" data-testid="comments-more" on:click=more>
                                            "Показать ещё"
                                        </button>
                                    }
                                })
                        }}
                    }
                        .into_any()
                }
            }}
            <Show when=move || writable.get()>
                <form method="post" onsubmit="return false" on:submit=submit data-testid="comment-form">
                    <textarea
                        rows="3"
                        placeholder="Новый комментарий"
                        aria-label="Новый комментарий"
                        data-testid="comment-text"
                        prop:value=move || draft.get()
                        on:input=move |ev| draft.set(event_target_value(&ev))
                    ></textarea>
                    <p>
                        <button type="submit" class="btn btn--primary" data-testid="comment-submit" disabled=move || busy.get()>
                            "Отправить"
                        </button>
                    </p>
                </form>
            </Show>
        </section>
    }
}
