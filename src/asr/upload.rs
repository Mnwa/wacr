use hyper::{Client, Request};
use hyper_multipart_rfc7578::client::multipart;
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
            let https = hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .https_only()
                .enable_http2()
                .build();

            let client = Client::builder().http2_only(true).build(https);

            let mut form = multipart::Form::default();
            form.add_file("file", self.audio_path)?;

            let req_builder = Request::post(self.upload_url.as_str());
            let req = form.set_body::<multipart::Body>(req_builder).unwrap();

            let body = client
                .request(req)
                .await
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
                .into_body();

            let body = hyper::body::to_bytes(body)
                .await
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

            let response = String::from_utf8(body.into())
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

            Ok(response)
        })
    }
}
