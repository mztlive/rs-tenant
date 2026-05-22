//! 面向租户级和平台级授权的 Axum 集成工具。

use std::future::poll_fn;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::cache::Cache;
use crate::decision::AccessDecision;
use crate::engine::Engine;
use crate::permission::Permission;
#[cfg(feature = "platform")]
use crate::platform::{
    PlatformAccessRequest, PlatformAuthorizationSource, PlatformEngine, PlatformPrincipalId,
    PlatformSubject,
};
use crate::request::{AuthSubject, TenantAccessRequest};
use crate::source::AuthorizationSource;
use crate::{PrincipalId, ScopePath, ScopedAccessRequest, TenantId};

use ::axum::body::Body;
use ::axum::http::{Request, StatusCode};
use ::axum::response::{IntoResponse, Response};
use ::tower::{Layer, Service};

/// 从请求中提取的认证上下文。
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// 租户级主体。
    pub subject: AuthSubject,
}

impl AuthContext {
    /// 创建认证上下文。
    pub fn new(tenant: TenantId, principal: PrincipalId) -> Self {
        Self {
            subject: AuthSubject::new(tenant, principal),
        }
    }
}

/// 从请求中提取的平台认证上下文。
#[cfg(feature = "platform")]
#[derive(Debug, Clone)]
pub struct PlatformAuthContext {
    /// 平台级主体。
    pub subject: PlatformSubject,
}

#[cfg(feature = "platform")]
impl PlatformAuthContext {
    /// 创建平台认证上下文。
    pub fn new(principal: PlatformPrincipalId) -> Self {
        Self {
            subject: PlatformSubject::new(principal),
        }
    }
}

/// 对租户级请求执行授权的中间件层。
#[derive(Debug, Clone)]
pub struct TenantAuthorizeLayer<S, C> {
    engine: Arc<Engine<S, C>>,
    permission: Permission,
}

impl<S, C> TenantAuthorizeLayer<S, C> {
    /// 创建租户授权中间件层。
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

    /// 将租户授权层应用到内层服务。
    fn layer(&self, inner: Inner) -> Self::Service {
        TenantAuthorizeService {
            inner,
            engine: self.engine.clone(),
            permission: self.permission.clone(),
        }
    }
}

/// 执行租户级权限检查的中间件服务。
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

    /// 标记中间件始终可以接收请求。
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// 授权通过后将请求转交给内层服务。
    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        let engine = self.engine.clone();
        let permission = self.permission.clone();

        Box::pin(async move {
            let subject = req
                .extensions()
                .get::<AuthContext>()
                .map(|context| context.subject.clone())
                .or_else(|| req.extensions().get::<AuthSubject>().cloned());
            let Some(subject) = subject else {
                return Ok((StatusCode::UNAUTHORIZED, "missing auth context").into_response());
            };

            match engine
                .can_tenant(TenantAccessRequest {
                    subject,
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

/// 对平台自有资源请求执行授权的中间件层。
#[cfg(feature = "platform")]
#[derive(Debug, Clone)]
pub struct PlatformAuthorizeLayer<S> {
    engine: Arc<PlatformEngine<S>>,
    permission: Permission,
}

#[cfg(feature = "platform")]
impl<S> PlatformAuthorizeLayer<S> {
    /// 创建平台授权中间件层。
    pub fn new(engine: Arc<PlatformEngine<S>>, permission: Permission) -> Self {
        Self { engine, permission }
    }
}

#[cfg(feature = "platform")]
impl<S, Inner> Layer<Inner> for PlatformAuthorizeLayer<S>
where
    S: PlatformAuthorizationSource,
{
    type Service = PlatformAuthorizeService<Inner, S>;

    /// 将平台授权层应用到内层服务。
    fn layer(&self, inner: Inner) -> Self::Service {
        PlatformAuthorizeService {
            inner,
            engine: self.engine.clone(),
            permission: self.permission.clone(),
        }
    }
}

/// 执行平台自有资源权限检查的中间件服务。
#[cfg(feature = "platform")]
#[derive(Debug, Clone)]
pub struct PlatformAuthorizeService<Inner, S> {
    inner: Inner,
    engine: Arc<PlatformEngine<S>>,
    permission: Permission,
}

#[cfg(feature = "platform")]
impl<Inner, S> Service<Request<Body>> for PlatformAuthorizeService<Inner, S>
where
    Inner: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    Inner::Future: Send + 'static,
    S: PlatformAuthorizationSource + 'static,
{
    type Response = Response;
    type Error = Inner::Error;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<Response, Self::Error>> + Send>>;

    /// 标记中间件始终可以接收请求。
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// 授权通过后将请求转交给内层服务。
    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        let engine = self.engine.clone();
        let permission = self.permission.clone();

        Box::pin(async move {
            let subject = req
                .extensions()
                .get::<PlatformAuthContext>()
                .map(|context| context.subject.clone())
                .or_else(|| req.extensions().get::<PlatformSubject>().cloned());
            let Some(subject) = subject else {
                return Ok(
                    (StatusCode::UNAUTHORIZED, "missing platform auth context").into_response()
                );
            };

            match engine
                .can_platform(PlatformAccessRequest {
                    subject,
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

/// 使用显式目标路径检查范围级请求。
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

/// 检查平台自有资源请求。
#[cfg(feature = "platform")]
pub async fn can_platform<S>(
    engine: &PlatformEngine<S>,
    subject: PlatformSubject,
    permission: Permission,
) -> crate::Result<AccessDecision>
where
    S: PlatformAuthorizationSource,
{
    engine
        .can_platform(PlatformAccessRequest {
            subject,
            permission,
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

    /// JWT 认证辅助逻辑返回的错误。
    #[derive(Debug, Error)]
    pub enum AuthError {
        /// 缺少 Authorization 请求头。
        #[error("missing authorization header")]
        MissingAuthorization,
        /// Authorization 请求头格式非法。
        #[error("invalid authorization header")]
        InvalidAuthorization,
        /// JWT 校验失败。
        #[error("invalid token")]
        InvalidToken,
        /// 必需声明缺失或非法。
        #[error("invalid claims: {0}")]
        InvalidClaims(String),
        /// 标识符非法。
        #[error("invalid id: {0}")]
        InvalidId(String),
    }

    /// Axum 提取器使用的拒绝类型。
    #[derive(Debug)]
    pub struct AuthRejection {
        status: StatusCode,
        message: String,
    }

    impl From<AuthError> for AuthRejection {
        /// 将认证错误转换成 Axum 拒绝类型。
        fn from(err: AuthError) -> Self {
            Self {
                status: StatusCode::UNAUTHORIZED,
                message: err.to_string(),
            }
        }
    }

    impl IntoResponse for AuthRejection {
        /// 将拒绝类型转换成 HTTP 响应。
        fn into_response(self) -> Response {
            (self.status, self.message).into_response()
        }
    }

    /// 用于从 JWT 中提取租户和主体标识符的声明类型。
    pub trait JwtClaims: DeserializeOwned + Send + Sync + Clone + 'static {
        /// 返回租户标识符字符串。
        fn tenant_id(&self) -> &str;
        /// 返回主体标识符字符串。
        fn principal_id(&self) -> &str;
    }

    /// 默认 JWT 声明结构：`{ tenant_id, principal_id }`。
    #[derive(Debug, Clone, serde::Deserialize)]
    pub struct DefaultClaims {
        /// 租户标识符。
        pub tenant_id: String,
        /// 主体标识符。
        pub principal_id: String,
        /// 标准 JWT 主题字段。
        pub sub: Option<String>,
        /// 标准 JWT 过期时间。
        pub exp: Option<usize>,
    }

    impl JwtClaims for DefaultClaims {
        /// 返回默认声明中的租户标识符。
        fn tenant_id(&self) -> &str {
            &self.tenant_id
        }

        /// 返回默认声明中的主体标识符。
        fn principal_id(&self) -> &str {
            &self.principal_id
        }
    }

    /// 持有解码配置的 JWT 认证状态。
    #[derive(Clone)]
    pub struct JwtAuthState<C: JwtClaims> {
        decoding_key: Arc<DecodingKey>,
        validation: Validation,
        _marker: PhantomData<fn() -> C>,
    }

    impl<C: JwtClaims> fmt::Debug for JwtAuthState<C> {
        /// 调试输出时隐藏解码密钥。
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("JwtAuthState")
                .field("decoding_key", &"<redacted>")
                .field("validation", &self.validation)
                .finish()
        }
    }

    impl<C: JwtClaims> JwtAuthState<C> {
        /// 创建 JWT 认证状态。
        pub fn new(decoding_key: DecodingKey, validation: Validation) -> Self {
            Self {
                decoding_key: Arc::new(decoding_key),
                validation,
                _marker: PhantomData,
            }
        }

        /// 从请求头中解码 JWT 并构造认证上下文。
        fn decode_from_headers(&self, headers: &HeaderMap) -> Result<JwtAuth<C>, AuthError> {
            let token = bearer_token(headers)?;
            let data = decode::<C>(&token, &self.decoding_key, &self.validation)
                .map_err(|_| AuthError::InvalidToken)?;
            JwtAuth::from_claims(data.claims)
        }
    }

    /// 为提取器提供 [`JwtAuthState`] 访问能力。
    pub trait JwtAuthProvider<C: JwtClaims> {
        /// 返回用于解码的 JWT 认证状态。
        fn jwt_auth(&self) -> &JwtAuthState<C>;
    }

    /// 已提取的 JWT 认证上下文和声明。
    #[derive(Debug, Clone)]
    pub struct JwtAuth<C: JwtClaims> {
        /// 解析出的认证上下文。
        pub context: AuthContext,
        /// 完整声明。
        pub claims: C,
    }

    impl<C: JwtClaims> JwtAuth<C> {
        /// 从声明中解析租户和主体标识符。
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

        /// 从请求部件中提取或复用 JWT 认证结果。
        async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
            if let Some(existing) = parts.extensions.get::<JwtAuth<C>>() {
                return Ok(existing.clone());
            }
            let auth = state.jwt_auth().decode_from_headers(&parts.headers)?;
            parts.extensions.insert(auth.context.subject.clone());
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

        /// 使用默认声明从请求部件中提取认证上下文。
        async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
            let auth = JwtAuth::<DefaultClaims>::from_request_parts(parts, state).await?;
            Ok(auth.context)
        }
    }

    /// 解码 JWT 并把认证上下文写入请求扩展的中间件层。
    #[derive(Debug, Clone)]
    pub struct JwtAuthLayer<C: JwtClaims> {
        state: Arc<JwtAuthState<C>>,
    }

    impl<C: JwtClaims> JwtAuthLayer<C> {
        /// 创建 JWT 认证中间件层。
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

        /// 将 JWT 认证层应用到内层服务。
        fn layer(&self, inner: S) -> Self::Service {
            JwtAuthService {
                inner,
                state: self.state.clone(),
            }
        }
    }

    /// 解码 JWT 并附加 [`AuthContext`] 的中间件服务。
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

        /// 标记中间件始终可以接收请求。
        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        /// 解码请求中的 JWT，并在成功后调用内层服务。
        fn call(&mut self, mut req: Request<Body>) -> Self::Future {
            let state = self.state.clone();
            let mut inner = self.inner.clone();

            Box::pin(async move {
                match state.decode_from_headers(req.headers()) {
                    Ok(auth) => {
                        req.extensions_mut().insert(auth.context.subject.clone());
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

    /// 从 Authorization 请求头中提取 Bearer 令牌。
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

#[cfg(all(test, feature = "platform", feature = "memory-store"))]
mod tests {
    use super::*;
    use crate::platform::{
        MemoryPlatformSource, PlatformEngineBuilder, PlatformGrantScope, PlatformPrincipalStatus,
        PlatformRoleId,
    };
    use futures::executor::block_on;
    use std::convert::Infallible;
    use std::future::{Ready, ready};

    #[derive(Clone)]
    /// 用于中间件测试的成功响应服务。
    struct OkService;

    impl Service<Request<Body>> for OkService {
        type Response = Response;
        type Error = Infallible;
        type Future = Ready<Result<Response, Self::Error>>;

        /// 测试服务始终可用。
        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        /// 返回无内容成功响应。
        fn call(&mut self, _req: Request<Body>) -> Self::Future {
            ready(Ok(StatusCode::NO_CONTENT.into_response()))
        }
    }

    /// 构造带平台权限的测试引擎和主体。
    fn platform_engine() -> (PlatformEngine<MemoryPlatformSource>, PlatformSubject) {
        let source = MemoryPlatformSource::new();
        let subject = PlatformAuthContext::new(
            PlatformPrincipalId::parse("platform_admin").expect("principal"),
        )
        .subject;
        let role = PlatformRoleId::parse("platform_admin").expect("role");
        source.set_principal_status(subject.principal.clone(), PlatformPrincipalStatus::Active);
        source.add_role_assignment(
            subject.principal.clone(),
            role.clone(),
            PlatformGrantScope::platform(),
        );
        source.add_role_permission(
            role,
            Permission::parse("platform/role:update").expect("permission"),
        );

        (PlatformEngineBuilder::new(source).build(), subject)
    }

    #[test]
    fn platform_authorize_layer_should_allow_platform_subject_extension() {
        let (engine, subject) = platform_engine();
        let layer = PlatformAuthorizeLayer::new(
            Arc::new(engine),
            Permission::parse("platform/role:update").expect("permission"),
        );
        let mut service = layer.layer(OkService);
        let mut req = Request::new(Body::empty());
        req.extensions_mut().insert(subject);

        let response = block_on(service.call(req)).expect("response");

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[test]
    fn platform_authorize_layer_should_accept_platform_auth_context() {
        let (engine, subject) = platform_engine();
        let layer = PlatformAuthorizeLayer::new(
            Arc::new(engine),
            Permission::parse("platform/role:update").expect("permission"),
        );
        let mut service = layer.layer(OkService);
        let mut req = Request::new(Body::empty());
        req.extensions_mut().insert(PlatformAuthContext { subject });

        let response = block_on(service.call(req)).expect("response");

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[test]
    fn platform_authorize_layer_should_reject_missing_context() {
        let (engine, _) = platform_engine();
        let layer = PlatformAuthorizeLayer::new(
            Arc::new(engine),
            Permission::parse("platform/role:update").expect("permission"),
        );
        let mut service = layer.layer(OkService);

        let response = block_on(service.call(Request::new(Body::empty()))).expect("response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn platform_authorize_layer_should_reject_denied_permission() {
        let (engine, subject) = platform_engine();
        let layer = PlatformAuthorizeLayer::new(
            Arc::new(engine),
            Permission::parse("platform/role:delete").expect("permission"),
        );
        let mut service = layer.layer(OkService);
        let mut req = Request::new(Body::empty());
        req.extensions_mut().insert(subject);

        let response = block_on(service.call(req)).expect("response");

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
