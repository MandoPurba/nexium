use actix_governor::{
    Governor, GovernorConfigBuilder, KeyExtractor, SimpleKeyExtractionError,
    governor::middleware::NoOpMiddleware,
};
use actix_web::dev::ServiceRequest;

pub fn ip_rate_limiter(requests_per_minute: u64) -> Governor<PeerIpKey, NoOpMiddleware> {
    let ms_per_request = 60_000 / requests_per_minute;
    let config = GovernorConfigBuilder::default()
        .milliseconds_per_request(ms_per_request)
        .burst_size(requests_per_minute as u32)
        .key_extractor(PeerIpKey)
        .finish()
        .expect("governor config");
    Governor::new(&config)
}

#[derive(Debug, Clone, Copy)]
pub struct PeerIpKey;

impl KeyExtractor for PeerIpKey {
    type Key = String;
    type KeyExtractionError = SimpleKeyExtractionError<&'static str>;

    fn extract(&self, req: &ServiceRequest) -> Result<Self::Key, Self::KeyExtractionError> {
        let ip = req
            .connection_info()
            .realip_remote_addr()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        Ok(ip)
    }
}
