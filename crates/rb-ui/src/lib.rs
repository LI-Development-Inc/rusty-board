use askama::Template;
use rb_core::models::{Board, Thread, Post};

#[derive(Template)]
#[template(path = "thread.html")]
pub struct ThreadTemplate<'a> {
    pub board: &'a Board,
    pub thread: &'a Thread,
    pub posts: &'a Vec<Post>,
    pub title: &'a str,
    pub media_url: &'a str,
    pub thumb_url: &'a str,
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate<'a> {
    pub board: &'a Board,
    pub threads: &'a Vec<(Thread, Post)>,
    pub title: &'a str,
}