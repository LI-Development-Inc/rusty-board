// rusty-board/crates/rb-ui/src/lib.rs
// This module defines the templates for rendering the HTML pages of the imageboard.
use askama::Template;
use rb_core::models::{Board, Thread, Post};

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate<'a> {
    pub board: &'a Board,
    pub threads: &'a Vec<(Thread, Post)>,
    // We pass the title as a plain String
    pub title: String, 
}

#[derive(Template)]
#[template(path = "thread.html")]
pub struct ThreadTemplate<'a> {
    pub board: &'a Board,
    pub thread: &'a Thread,
    pub posts: &'a Vec<Post>,
    pub title: String,
    pub media_url: String,
    pub thumb_url: String,
}

#[derive(Template)]
#[template(path = "catalog.html")]
pub struct CatalogTemplate<'a> {
    pub board: &'a Board,
    pub threads: &'a Vec<(Thread, Post)>,
    pub title: String,
}