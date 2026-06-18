use actix_web::{HttpResponse, get};
use prometheus::{Encoder, TextEncoder};

#[get("/metrics")]
pub async fn metrics_handler() -> HttpResponse {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    if encoder.encode(&metric_families, &mut buffer).is_err() {
        return HttpResponse::InternalServerError().finish();
    }
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4; charset=utf-8")
        .body(buffer)
}
