use axum::{Json, Router, routing::get};
use client_contracts::{
    ClientChange, ClientChangesPage, ClientDetailDto, ClientDto, ClientSnapshot, ContactDto,
    ContactSnapshot, CreateClientRequest, CreateContactRequest, Entity, PositionDto,
    PositionRequest, UpdateClientRequest, UpdateContactRequest,
};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

use super::{AppState, clients, contacts, internal, positions};

#[derive(OpenApi)]
#[openapi(
    info(title = "Client API", version = "0.1.0"),
    paths(
        clients::create,
        clients::list,
        clients::get_one,
        clients::update,
        clients::add_contact,
        contacts::update,
        contacts::remove,
        positions::list,
        positions::create,
        positions::rename,
        positions::remove,
        internal::client_changes,
        platform_core::health::health,
    ),
    components(schemas(
        ClientDto,
        ClientDetailDto,
        ContactDto,
        PositionDto,
        PositionRequest,
        CreateClientRequest,
        UpdateClientRequest,
        CreateContactRequest,
        UpdateContactRequest,
        ClientChange,
        ClientChangesPage,
        ClientSnapshot,
        ContactSnapshot,
        Entity,
        platform_core::error::ProblemDetails,
    )),
    modifiers(&Security)
)]
struct ApiDoc;

struct Security;

impl Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "service",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/openapi.json", get(|| async { Json(ApiDoc::openapi()) }))
}
