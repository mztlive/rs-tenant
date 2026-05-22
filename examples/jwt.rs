use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use rs_tenant::axum::jwt::{DefaultClaims, JwtAuthLayer, JwtAuthState};

fn main() {
    let validation = Validation::new(Algorithm::HS256);
    let state = JwtAuthState::<DefaultClaims>::new(
        DecodingKey::from_secret(b"replace-with-application-secret"),
        validation,
    );
    let _layer = JwtAuthLayer::new(state);
}
