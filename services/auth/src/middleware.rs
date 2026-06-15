//! Actix middleware that validates a `Bearer <jwt>` `Authorization` header,
//! decodes the token via the [`JwtIssuer`] stored in app data, and inserts
//! the resulting [`Claims`] into the request's extensions for the
//! [`AuthUser`](crate::extractors::AuthUser) extractor to read.

use std::future::{Future, Ready, ready};
use std::pin::Pin;

use actix_web::{
    Error, HttpMessage, HttpResponse,
    body::EitherBody,
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
    http::header::AUTHORIZATION,
    web,
};
use serde_json::json;

use crate::jwt::JwtIssuer;

type BoxedFuture<T> = Pin<Box<dyn Future<Output = T>>>;

pub struct JwtAuth;

impl<S, B> Transform<S, ServiceRequest> for JwtAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = JwtAuthMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(JwtAuthMiddleware { service }))
    }
}

pub struct JwtAuthMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for JwtAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = BoxedFuture<Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let token = req
            .headers()
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(str::to_string);

        let token = match token {
            Some(t) => t,
            None => return Box::pin(reject(req, "missing or malformed Authorization header")),
        };

        let issuer = req.app_data::<web::Data<JwtIssuer>>().cloned();
        let issuer = match issuer {
            Some(i) => i,
            None => return Box::pin(reject(req, "auth misconfigured")),
        };

        let claims = match issuer.verify(&token) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(error = %e, "jwt verify failed");
                return Box::pin(reject(req, "invalid or expired token"));
            }
        };

        req.extensions_mut().insert(claims);

        let fut = self.service.call(req);
        Box::pin(async move { fut.await.map(ServiceResponse::map_into_left_body) })
    }
}

async fn reject<B>(
    req: ServiceRequest,
    message: &'static str,
) -> Result<ServiceResponse<EitherBody<B>>, Error> {
    let resp = HttpResponse::Unauthorized().json(json!({
        "code": "UNAUTHORIZED",
        "message": message,
    }));
    Ok(req.into_response(resp.map_into_right_body()))
}
