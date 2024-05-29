use tower::Layer;

use crate::json_error::private::JsonErrorService;

#[derive(Debug, Clone)]
#[must_use]
pub struct JsonError;

impl<S> Layer<S> for JsonError {
    type Service = JsonErrorService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        JsonErrorService { inner }
    }
}

mod private {
    use std::task::Context;

    use http::Request;
    use tower_service::Service;

    #[derive(Debug, Clone, Copy)]
    pub struct JsonErrorService<S> {
        pub(super) inner: S,
    }

    impl<B, S> Service<Request<B>> for JsonErrorService<S>
        where
            S: Service<Request<B>>,
    {
        type Response = S::Response;
        type Error = S::Error;
        type Future = S::Future;

        #[inline]
        fn poll_ready(&mut self, cx: &mut Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
            self.inner.poll_ready(cx)
        }

        #[inline]
        fn call(&mut self, mut req: Request<B>) -> Self::Future {
            let response = self.inner.call(req);
            response
        }
    }
}