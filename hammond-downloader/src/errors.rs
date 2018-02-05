use hammond_data::errors::DataError;
use reqwest;
use std::io;

#[derive(Fail, Debug)]
pub enum DownloadError {
    #[fail(display = "Reqwest error: {}", _0)]
    RequestError(#[cause] reqwest::Error),
    #[fail(display = "Data error: {}", _0)]
    DataError(#[cause] DataError),
    #[fail(display = "Io error: {}", _0)]
    IoError(#[cause] io::Error),
    #[fail(display = "The Download was cancelled")]
    DownloadCancelled,
    #[fail(display = "Unexpected server response: {}", _0)]
    UnexpectedResponse(reqwest::StatusCode),
}

impl From<reqwest::Error> for DownloadError {
    fn from(err: reqwest::Error) -> Self {
        DownloadError::RequestError(err)
    }
}

impl From<io::Error> for DownloadError {
    fn from(err: io::Error) -> Self {
        DownloadError::IoError(err)
    }
}

impl From<DataError> for DownloadError {
    fn from(err: DataError) -> Self {
        DownloadError::DataError(err)
    }
}
