//! Reading-progress writes: position, position count, finished state.

use crate::bookshelf::{blocking, Bookshelf};
use crate::error::InkunaError;

#[uniffi::export(async_runtime = "tokio")]
impl Bookshelf {
    /// One call per page turn. `locator` is the opaque Readium locator
    /// JSON; `progression` the book-wide totalProgression; `position` the
    /// synthetic position, once the navigator knows it.
    pub async fn update_progress(
        &self,
        id: String,
        locator: String,
        progression: f64,
        position: Option<u32>,
    ) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.update_progress(&id, &locator, progression, position)?)).await
    }

    /// Once per book, after the navigator computes synthetic positions;
    /// from then on "page N of M" is real.
    pub async fn report_position_count(&self, id: String, count: u32) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.report_position_count(&id, count)?)).await
    }

    /// Explicit finish/unfinish; unfinishing sticks at end-of-book because
    /// auto-finish only fires on an upward crossing of the threshold.
    pub async fn set_finished(&self, id: String, finished: bool) -> Result<(), InkunaError> {
        let library = self.0.clone();
        blocking(move || Ok(library.set_finished(&id, finished)?)).await
    }
}
