//! Rendering a read once and cutting every page from that render is the
//! same projection as rendering it again for each page (frozen
//! continuations rely on it byte for byte).

use kmp_proto::v1beta1::{AskRequest, MemoryBudget, PageRequest, WakeRequest};

use super::test_support::{typed_ask_fixture, typed_wake_fixture, wake_request_with_bytes};
use super::typed_recall::{
    project_ask_response, project_rendered_ask, project_rendered_wake, project_wake_response,
    render_ask, render_wake,
};

fn next_cursor(projection: Option<&kmp_proto::v1beta1::RecallProjection>) -> Option<String> {
    projection?.page.as_ref()?.next_cursor.clone()
}

fn with_cursor(page: &mut Option<PageRequest>, cursor: String) {
    page.get_or_insert_with(PageRequest::default).cursor = cursor;
}

#[test]
fn every_wake_page_cut_from_one_render_equals_a_fresh_projection() {
    let response = typed_wake_fixture(24);
    let mut request = WakeRequest {
        dimensions: Some(Default::default()),
        ..wake_request_with_bytes(4_000)
    };
    let (rendered_response, rendered) = render_wake(response.clone(), &request);
    let mut pages = 0;
    loop {
        let fresh = project_wake_response(response.clone(), &request).expect("fresh page");
        let cut = project_rendered_wake(rendered_response.clone(), rendered.clone(), &request)
            .expect("page cut from the render");
        assert_eq!(cut, fresh, "page {pages}");
        pages += 1;
        let Some(cursor) = next_cursor(fresh.projection.as_ref()) else {
            break;
        };
        with_cursor(&mut request.page, cursor);
        assert!(pages < 200, "the fixture must end");
    }
    assert!(pages > 2, "the fixture must page: {pages}");
}

#[test]
fn every_ask_page_cut_from_one_render_equals_a_fresh_projection() {
    let response = typed_ask_fixture(24);
    let mut request = AskRequest {
        about: "project:kmp".into(),
        question: "What is current?".into(),
        budget: Some(MemoryBudget {
            max_bytes: 2_400,
            ..MemoryBudget::default()
        }),
        ..AskRequest::default()
    };
    let rendered = render_ask(&response);
    let mut pages = 0;
    loop {
        let fresh = project_ask_response(response.clone(), &request).expect("fresh page");
        let cut = project_rendered_ask(response.clone(), rendered.clone(), &request)
            .expect("page cut from the render");
        assert_eq!(cut, fresh, "page {pages}");
        pages += 1;
        let Some(cursor) = next_cursor(fresh.projection.as_ref()) else {
            break;
        };
        with_cursor(&mut request.page, cursor);
        assert!(pages < 200, "the fixture must end");
    }
    assert!(pages > 1, "the fixture must page: {pages}");
}
