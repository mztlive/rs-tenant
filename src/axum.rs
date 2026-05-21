//! Axum integration utilities for tenant-scoped authorization.

use std::future::poll_fn;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::cache::Cache;
use crate::decision::AccessDecision;
use crate::engine::Engine;
use crate::permission::Permission;
use crate::request::{AuthSubject, TenantAccessRequest};
use crate::source::AuthorizationSource;
use crate::{PrincipalId, ScopePath, ScopedAccessRequest, TenantId};

use ::axum::body::Body;
use ::axum::http::{Request, StatusCode};
use ::axum::response::{IntoResponse, Response};
use ::tower::{Layer, Service};

/// Authentication context extracted from a request.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// Tenant-scoped subject.
    pub subject: AuthSubject,
}

impl AuthContext {
    /// Creates an auth context.
    pub fn new(tenant: TenantId, principal: PrincipalId) -> Self {
        Self {
            subject: AuthSubject::new(tenant, principal),
        }
    }
}

/// Middleware layer that authorizes tenant-level requests.
#[derive(Debug, Clone)]
pub struct TenantAuthorizeLayer<S, C> {
    engine: Arc<Engine<S, C>>,
    permission: Permission,
}

impl<S, C> TenantAuthorizeLayer<S, C> {
    /// Creates a new tenant authorization layer.
    pub fn new(engine: Arc<Engine<S, C>>, permission: Permission) -> Self {
        Self { engine, permission }
    }
}

impl<S, C, Inner> Layer<Inner> for TenantAuthorizeLayer<S, C>
where
    S: AuthorizationSource,
    C: Cache,
{
    type Service = TenantAuthorizeService<Inner, S, C>;

    fn layer(&self, inner: Inner) -> Self::Service {
        TenantAuthorizeService {
            inner,
            engine: self.engine.clone(),
            permission: self.permission.clone(),
        }
    }
}

/// Middleware service that enforces tenant-level permission checks.
#[derive(Debug, Clone)]
pub struct TenantAuthorizeService<Inner, S, C> {
    inner: Inner,
    engine: Arc<Engine<S, C>>,
    permission: Permission,
}

impl<Inner, S, C> Service<Request<Body>> for TenantAuthorizeService<Inner, S, C>
where
    Inner: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    Inner::Future: Send + 'static,
    S: AuthorizationSource + 'static,
    C: Cache + 'static,
{
    type Response = Response;
    type Error = Inner::Error;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        let engine = self.engine.clone();
        let permission = self.permission.clone();

        Box::pin(async move {
            let context = req.extensions().get::<AuthContext>().cloned();
            let Some(context) = context else {
                return Ok((StatusCode::UNAUTHORIZED, "missing auth context").into_response());
            };

            match engine
                .can_tenant(TenantAccessRequest {
                    subject: context.subject,
                    permission,
                })
                .await
            {
                Ok(AccessDecision::Allow) => {
                    poll_fn(|cx| inner.poll_ready(cx)).await?;
                    inner.call(req).await
                }
                Ok(AccessDecision::Deny) => {
                    Ok((StatusCode::FORBIDDEN, "forbidden").into_response())
                }
                Err(_) => Ok((StatusCode::INTERNAL_SERVER_ERROR, "auth error").into_response()),
            }
        })
    }
}

/// Checks a scoped request with an explicit target path.
pub async fn can_access_scope<S, C>(
    engine: &Engine<S, C>,
    subject: AuthSubject,
    permission: Permission,
    target: ScopePath,
) -> crate::Result<AccessDecision>
where
    S: AuthorizationSource,
    C: Cache,
{
    engine
        .can_access_scope(ScopedAccessRequest {
            subject,
            permission,
            target,
        })
        .await
}

#[cfg(feature = "axum-jwt")]
pub mod jwt {
    use std::fmt;
    use std::future::poll_fn;
    use std::marker::PhantomData;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::task::{Context, Poll};

    use jsonwebtoken::{DecodingKey, Validation, decode};
    use serde::de::DeserializeOwned;
    use thiserror::Error;

    use crate::axum::AuthContext;
    use crate::{PrincipalId, TenantId};

    use ::axum::body::Body;
    use ::axum::extract::FromRequestParts;
    use ::axum::http::header::AUTHORIZATION;
    use ::axum::http::request::Parts;
    use ::axum::http::{HeaderMap, Request, StatusCode};
    use ::axum::response::{IntoResponse, Response};
    use ::tower::{Layer, Service};

    /// Errors returned by JWT auth helpers.
    #[derive(Debug, Error)]
    pub enum AuthError {
        /// Authorization header is missing.
        #[error("missing authorization header")]
        MissingAuthorization,
        /// Authorization header format is invalid.
        #[error("invalid authorization header")]
        InvalidAuthorization,
        /// JWT validation error.
        #[error("invalid token")]
        InvalidToken,
        /// Required claims are missing or invalid.
        #[error("invalid claims: {0}")]
        InvalidClaims(String),
        /// Invalid identifier.
        #[error("invalid id: {0}")]
        InvalidId(String),
    }

    /// Rejection type for axum extractors.
    #[derive(Debug)]
    pub struct AuthRejection {
        status: StatusCode,
        message: String,
    }

    impl From<AuthError> for AuthRejection {
        fn from(err: AuthError) -> Self {
            Self {
                status: StatusCode::UNAUTHORIZED,
                message: err.to_string(),
            }
        }
    }

    impl IntoResponse for AuthRejection {
        fn into_response(self) -> Response {
            (self.status, self.message).into_response()
        }
    }

    /// Claims type used to extract tenant/principal identifiers from JWTs.
    pub trait JwtClaims: DeserializeOwned + Send + Sync + Clone + 'static {
        /// Returns the tenant identifier string.
        fn tenant_id(&self) -> &str;
        /// Returns the principal identifier string.
        fn principal_id(&self) -> &str;
    }

    /// Default JWT claims shape: `{ tenant_id, principal_id }`.
    #[derive(Debug, Clone, serde::Deserialize)]
    pub struct DefaultClaims {
        /// Tenant identifier.
        pub tenant_id: String,
        /// Principal identifier.
        pub principal_id: String,
        /// Standard JWT subject.
        pub sub: Option<String>,
        /// Standard JWT expiration.
        pub exp: Option<usize>,
    }

    impl JwtClaims for DefaultClaims {
        fn tenant_id(&self) -> &str {
            &self.tenant_id
        }

        fn principal_id(&self) -> &str {
            &self.principal_id
        }
    }

    /// JWT auth state holding decoding settings.
    #[derive(Clone)]
    pub struct JwtAuthState<C: JwtClaims> {
        decoding_key: Arc<DecodingKey>,
        validation: Validation,
        _marker: PhantomData<fn() -> C>,
    }

    impl<C: JwtClaims> fmt::Debug for JwtAuthState<C> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("JwtAuthState")
                .field("decoding_key", &"<redacted>")
                .field("validation", &self.validation)
                .finish()
        }
    }

    impl<C: JwtClaims> JwtAuthState<C> {
        /// Creates a new JWT auth state.
        pub fn new(decoding_key: DecodingKey, validation: Validation) -> Self {
            Self {
                decoding_key: Arc::new(decoding_key),
                validation,
                _marker: PhantomData,
            }
        }

        fn decode_from_headers(&self, headers: &HeaderMap) -> Result<JwtAuth<C>, AuthError> {
            let token = bearer_token(headers)?;
            let data = decode::<C>(&token, &self.decoding_key, &self.validation)
                .map_err(|_| AuthError::InvalidToken)?;
            JwtAuth::from_claims(data.claims)
        }
    }

    /// Provides access to [`JwtAuthState`] for extractors.
    pub trait JwtAuthProvider<C: JwtClaims> {
        /// Returns the JWT auth state for decoding.
        fn jwt_auth(&self) -> &JwtAuthState<C>;
    }

    /// Extracted JWT auth context plus claims.
    #[derive(Debug, Clone)]
    pub struct JwtAuth<C: JwtClaims> {
        /// Parsed auth context.
        pub context: AuthContext,
        /// Full claims.
        pub claims: C,
    }

    impl<C: JwtClaims> JwtAuth<C> {
        fn from_claims(claims: C) -> Result<Self, AuthError> {
            let tenant = TenantId::parse(claims.tenant_id())
                .map_err(|err| AuthError::InvalidId(err.to_string()))?;
            let principal = PrincipalId::parse(claims.principal_id())
                .map_err(|err| AuthError::InvalidId(err.to_string()))?;
            Ok(Self {
                context: AuthContext::new(tenant, principal),
                claims,
            })
        }
    }

    impl<S, C> FromRequestParts<S> for JwtAuth<C>
    where
        S: Send + Sync + JwtAuthProvider<C>,
        C: JwtClaims,
    {
        type Rejection = AuthRejection;

        async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
            if let Some(existing) = parts.extensions.get::<JwtAuth<C>>() {
                return Ok(existing.clone());
            }
            let auth = state.jwt_auth().decode_from_headers(&parts.headers)?;
            parts.extensions.insert(auth.clone());
            parts.extensions.insert(auth.context.clone());
            Ok(auth)
        }
    }

    impl<S> FromRequestParts<S> for AuthContext
    where
        S: Send + Sync + JwtAuthProvider<DefaultClaims>,
    {
        type Rejection = AuthRejection;

        async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
            let auth = JwtAuth::<DefaultClaims>::from_request_parts(parts, state).await?;
            Ok(auth.context)
        }
    }

    /// Middleware layer that decodes JWT and inserts auth context into request extensions.
    #[derive(Debug, Clone)]
    pub struct JwtAuthLayer<C: JwtClaims> {
        state: Arc<JwtAuthState<C>>,
    }

    impl<C: JwtClaims> JwtAuthLayer<C> {
        /// Creates a new JWT auth layer.
        pub fn new(state: JwtAuthState<C>) -> Self {
            Self {
                state: Arc::new(state),
            }
        }
    }

    impl<S, C> Layer<S> for JwtAuthLayer<C>
    where
        C: JwtClaims,
    {
        type Service = JwtAuthService<S, C>;

        fn layer(&self, inner: S) -> Self::Service {
            JwtAuthService {
                inner,
                state: self.state.clone(),
            }
        }
    }

    /// Middleware service that decodes JWT and attaches [`AuthContext`].
    #[derive(Debug, Clone)]
    pub struct JwtAuthService<S, C: JwtClaims> {
        inner: S,
        state: Arc<JwtAuthState<C>>,
    }

    impl<S, C> Service<Request<Body>> for JwtAuthService<S, C>
    where
        S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
        S::Future: Send + 'static,
        C: JwtClaims,
    {
        type Response = Response;
        type Error = S::Error;
        type Future =
            Pin<Box<dyn std::future::Future<Output = Result<Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, mut req: Request<Body>) -> Self::Future {
            let state = self.state.clone();
            let mut inner = self.inner.clone();

            Box::pin(async move {
                match state.decode_from_headers(req.headers()) {
                    Ok(auth) => {
                        req.extensions_mut().insert(auth.context.clone());
                        req.extensions_mut().insert(auth);
                        poll_fn(|cx| inner.poll_ready(cx)).await?;
                        inner.call(req).await
                    }
                    Err(err) => Ok(AuthRejection::from(err).into_response()),
                }
            })
        }
    }

    fn bearer_token(headers: &HeaderMap) -> Result<String, AuthError> {
        let value = headers
            .get(AUTHORIZATION)
            .ok_or(AuthError::MissingAuthorization)?;
        let value = value
            .to_str()
            .map_err(|_| AuthError::InvalidAuthorization)?;
        let token = value
            .strip_prefix("Bearer ")
            .ok_or(AuthError::InvalidAuthorization)?;
        if token.is_empty() {
            return Err(AuthError::InvalidAuthorization);
        }
        Ok(token.to_string())
    }
}
