use crate::support::*;

#[tokio::test]
async fn the_page_loads_the_datastar_client() {
    let page: String = text(send(&service().router, get("/")).await).await;
    assert!(page.contains(r#"<script type="module" src="https://cdn.jsdelivr.net/gh/starfederation/datastar@v1.0.5/bundles/datastar.js"></script>"#));
}

#[tokio::test]
async fn the_page_opens_the_event_stream_on_load() {
    let page: String = text(send(&service().router, get("/")).await).await;
    assert!(page.contains(r#"data-init="@get('/events')""#));
}

#[tokio::test]
async fn the_page_posts_the_title_as_a_form() {
    let page: String = text(send(&service().router, get("/")).await).await;
    assert!(page.contains(r#"<form data-on:submit="@post('/tasks', {contentType: 'form'})">"#));
}

#[tokio::test]
async fn the_page_bounds_the_title_at_the_domains_limit() {
    let page: String = text(send(&service().router, get("/")).await).await;
    assert!(page.contains(&format!(r#"<input name="title" required maxlength="{}""#, Title::LIMIT)));
}
