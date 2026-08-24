use inkuna_engine::EngineError;

use super::CoreError;

#[test]
fn engine_error_maps_to_core() {
    assert!(matches!(
        CoreError::from(EngineError::NotReady),
        CoreError::NotReady
    ));
    assert!(matches!(
        CoreError::from(EngineError::UnsupportedContent {
            detail: "fixed-layout".to_string(),
        }),
        CoreError::UnsupportedContent(detail) if detail == "fixed-layout"
    ));
    assert!(matches!(
        CoreError::from(EngineError::BudgetExceeded {
            detail: "pages".to_string(),
        }),
        CoreError::LayoutBudgetExceeded(detail) if detail == "pages"
    ));
    assert!(matches!(
        CoreError::from(EngineError::AnchorNotFound {
            detail: "#missing".to_string(),
        }),
        CoreError::AnchorNotFound(detail) if detail == "#missing"
    ));
    assert!(matches!(
        CoreError::from(EngineError::Io(std::io::Error::other("disk"))),
        CoreError::Io(_)
    ));
    assert!(matches!(
        CoreError::from(EngineError::Content(
            inkuna_content::ContentError::Archive("bad zip".to_string())
        )),
        CoreError::Archive(detail) if detail == "bad zip"
    ));
}
