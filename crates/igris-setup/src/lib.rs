//! `igris setup`: a local web app on 127.0.0.1. Every request passes one filter (`guard`), the
//! browser can only trigger actions from a fixed list (`action`), and the page is reached with a
//! one-time token that turns into a session cookie (`auth`).

pub mod action;
pub mod auth;
pub mod guard;

#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("no secure random source: {0}")]
    Random(String),
}
