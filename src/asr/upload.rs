use actix_multipart_rfc7578::client::multipart::{Body, Form};
use std::future::{Future, IntoFuture};
use std::path::PathBuf;
use std::pin::Pin;
use url::Url;

pub struct Uploader {
    pub audio_path: PathBuf,
    pub upload_url: Url,
}

impl IntoFuture for Uploader {
    type Output = std::io::Result<String>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output>>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let mut form = Form::default();
            form.add_file("file", self.audio_path)?;

            let body = awc::Client::new()
                .post(self.upload_url.as_str())
                .content_type(form.content_type())
                .send_body(Body::from(form))
                .await
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
                .body()
                .await
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

            let response = String::from_utf8(body.to_vec())
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

            Ok(response)
        })
    }
}
