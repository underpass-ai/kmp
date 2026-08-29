use std::future::Future;
use std::pin::Pin;

use crate::guide::application::dto::guide_request_document_dto::GuideRequestDocumentDto;
use crate::guide::domain::guide_error::GuideError;

pub trait GuideMemoryGateway {
    fn converge<'a>(
        &'a self,
        requests: &'a [GuideRequestDocumentDto],
    ) -> Pin<Box<dyn Future<Output = Result<(), GuideError>> + 'a>>;
}
