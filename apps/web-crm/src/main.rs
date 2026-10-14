use axum::Router;
use leptos::prelude::*;
use leptos_axum::{LeptosRoutes, generate_route_list};
use ui_kit::BffUrl;
use web_crm::{App, shell};

#[tokio::main]
async fn main() {
    let bff_url = std::env::var("BFF_URL").unwrap_or_else(|_| "http://127.0.0.1:8081".into());
    let conf = get_configuration(None).expect("leptos configuration");
    let addr = conf.leptos_options.site_addr;
    let options = conf.leptos_options;
    let routes = generate_route_list(App);

    let context_bff = bff_url.clone();
    let app = Router::new()
        .leptos_routes_with_context(
            &options,
            routes,
            move || provide_context(BffUrl(context_bff.clone())),
            {
                let options = options.clone();
                move || shell(options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(options);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("bind web-crm");
    println!("web-crm listening on http://{addr} (bff: {bff_url})");
    axum::serve(listener, app.into_make_service())
        .await
        .expect("serve web-crm");
}
