use axum::{
    body::Body, extract::State, http::{Request, StatusCode}, middleware::Next, response::Response
};

pub async fn api_key_middleware(
    State(keys): State<Vec<String>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if keys.is_empty() {
        return Ok(next.run(request).await);
    }

    if let Some(api_key) = request.headers().get("x-api-key") {
        if keys.iter().any(|key| key == api_key) {
            return Ok(next.run(request).await);
        }
    }

    Err(StatusCode::UNAUTHORIZED)
}
