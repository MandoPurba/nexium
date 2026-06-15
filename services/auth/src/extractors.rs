//! Request extractors.
//!
//! [`AuthUser`] pulls the verified JWT claims that [`JwtAuth`] middleware
//! placed into the request extensions. Using the extractor on a handler
//! mounted outside the wrapped scope yields `401 UNAUTHORIZED`.
//!
//! [`JwtAuth`]: crate::middleware::JwtAuth

use std::future::{Ready, ready};

use actix_web::{FromRequest, HttpMessage, HttpRequest, dev::Payload};
use uuid::Uuid;

use crate::error::ApiError;
use crate::jwt::Claims;

#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub id: Uuid,
}

impl FromRequest for AuthUser {
    type Error = ApiError;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        ready(
            req.extensions()
                .get::<Claims>()
                .map(|c| AuthUser { id: c.sub })
                .ok_or(ApiError::Unauthorized),
        )
    }
}
