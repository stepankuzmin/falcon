extern crate log;

use actix_web::{http, test, App};

use martin::dev::{mock_function_sources, mock_state, mock_table_sources};
use martin::function_source::FunctionSources;
use martin::server::router;
use martin::table_source::TableSources;

fn init() {
    let _ = env_logger::builder().is_test(true).try_init();
}

#[actix_rt::test]
async fn test_get_table_sources_ok() {
    init();

    let state = mock_state(mock_table_sources(), None, false);
    let mut app = test::init_service(App::new().data(state).configure(router)).await;

    let req = test::TestRequest::get().uri("/index.json").to_request();
    let response = test::call_service(&mut app, req).await;
    assert!(response.status().is_success());

    let body = test::read_body(response).await;
    let table_sources: TableSources = serde_json::from_slice(&body).unwrap();
    assert!(table_sources.contains_key("public.table_source"));
}

#[actix_rt::test]
async fn test_get_table_sources_watch_mode_ok() {
    init();

    let state = mock_state(mock_table_sources(), None, true);
    let mut app = test::init_service(App::new().data(state).configure(router)).await;

    let req = test::TestRequest::get().uri("/index.json").to_request();
    let response = test::call_service(&mut app, req).await;
    assert!(response.status().is_success());

    let body = test::read_body(response).await;
    let table_sources: TableSources = serde_json::from_slice(&body).unwrap();
    assert!(table_sources.contains_key("public.table_source"));
}

#[actix_rt::test]
async fn test_get_table_source_ok() {
    init();

    let state = mock_state(mock_table_sources(), None, false);
    let mut app = test::init_service(App::new().data(state).configure(router)).await;

    let req = test::TestRequest::get()
        .uri("/public.non_existant.json")
        .to_request();

    let response = test::call_service(&mut app, req).await;
    assert_eq!(response.status(), http::StatusCode::NOT_FOUND);

    let req = test::TestRequest::get()
        .uri("/public.table_source.json")
        .to_request();

    let response = test::call_service(&mut app, req).await;
    assert!(response.status().is_success());
}

#[actix_rt::test]
async fn test_get_table_source_tile_ok() {
    init();

    let state = mock_state(mock_table_sources(), None, false);
    let mut app = test::init_service(App::new().data(state).configure(router)).await;

    let req = test::TestRequest::get()
        .uri("/public.non_existant/0/0/0.pbf")
        .to_request();

    let response = test::call_service(&mut app, req).await;
    assert_eq!(response.status(), http::StatusCode::NOT_FOUND);

    let req = test::TestRequest::get()
        .uri("/public.table_source/0/0/0.pbf")
        .to_request();

    let response = test::call_service(&mut app, req).await;
    assert!(response.status().is_success());
}

#[actix_rt::test]
async fn test_get_function_sources_ok() {
    init();

    let state = mock_state(None, mock_function_sources(), false);
    let mut app = test::init_service(App::new().data(state).configure(router)).await;

    let req = test::TestRequest::get().uri("/rpc/index.json").to_request();
    let response = test::call_service(&mut app, req).await;
    assert!(response.status().is_success());

    let body = test::read_body(response).await;
    let function_sources: FunctionSources = serde_json::from_slice(&body).unwrap();
    assert!(function_sources.contains_key("public.function_source"));
}

#[actix_rt::test]
async fn test_get_function_sources_watch_mode_ok() {
    init();

    let state = mock_state(None, mock_function_sources(), true);
    let mut app = test::init_service(App::new().data(state).configure(router)).await;

    let req = test::TestRequest::get().uri("/rpc/index.json").to_request();
    let response = test::call_service(&mut app, req).await;
    assert!(response.status().is_success());

    let body = test::read_body(response).await;
    let function_sources: FunctionSources = serde_json::from_slice(&body).unwrap();
    assert!(function_sources.contains_key("public.function_source"));
}

#[actix_rt::test]
async fn test_get_function_source_ok() {
    init();

    let state = mock_state(None, mock_function_sources(), false);
    let mut app = test::init_service(App::new().data(state).configure(router)).await;

    let req = test::TestRequest::get()
        .uri("/rpc/public.non_existant.json")
        .to_request();

    let response = test::call_service(&mut app, req).await;
    assert_eq!(response.status(), http::StatusCode::NOT_FOUND);

    let req = test::TestRequest::get()
        .uri("/rpc/public.function_source.json")
        .to_request();

    let response = test::call_service(&mut app, req).await;
    assert!(response.status().is_success());
}

#[actix_rt::test]
async fn test_get_function_source_tile_ok() {
    init();

    let state = mock_state(None, mock_function_sources(), false);
    let mut app = test::init_service(App::new().data(state).configure(router)).await;

    let req = test::TestRequest::get()
        .uri("/rpc/public.function_source/0/0/0.pbf")
        .to_request();

    let response = test::call_service(&mut app, req).await;
    assert!(response.status().is_success());
}
