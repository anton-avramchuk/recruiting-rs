//! JWT-токены, роли и права, axum-extractor для аутентификации.
mod claims;
mod extractor;
pub mod jwks;
mod keys;
pub mod password;
mod role;
mod service;
mod token;

pub use claims::{Audience, Claims};
pub use extractor::AuthUser;
pub use keys::{KeyError, KeyPair, PublicKey};
pub use role::{Permission, Role};
pub use service::{ServiceCaller, ServiceToken};
pub use token::{AuthError, TokenSigner, TokenVerifier};
