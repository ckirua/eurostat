//! Dataset search helpers.

use crate::cache::Cache;
use crate::error::Result;
use crate::model::DatasetInfo;

#[cfg(feature = "fuzzy-search")]
use fuzzy_matcher::skim::SkimMatcherV2;
#[cfg(feature = "fuzzy-search")]
use fuzzy_matcher::FuzzyMatcher;

/// Search cached datasets using FTS and optional fuzzy ranking.
pub struct SearchEngine<'a> {
    cache: &'a Cache,
}

impl<'a> SearchEngine<'a> {
    /// Create a search engine over a cache instance.
    pub fn new(cache: &'a Cache) -> Self {
        Self { cache }
    }

    /// Search datasets by query string.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<DatasetInfo>> {
        let fts_query = query
            .split_whitespace()
            .map(Self::sanitize_fts_term)
            .filter(|term| !term.is_empty())
            .map(|term| format!("{term}*"))
            .collect::<Vec<_>>()
            .join(" OR ");

        if fts_query.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = self.cache.search(&fts_query, limit as i64).await?;

        #[cfg(feature = "fuzzy-search")]
        {
            if results.is_empty() {
                results = self.fuzzy_fallback(query, limit).await?;
            }
        }

        Ok(results)
    }

    fn sanitize_fts_term(term: &str) -> String {
        term.chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
            .collect()
    }

    #[cfg(feature = "fuzzy-search")]
    async fn fuzzy_fallback(&self, query: &str, limit: usize) -> Result<Vec<DatasetInfo>> {
        let matcher = SkimMatcherV2::default();
        let mut scored = self
            .cache
            .all_datasets()
            .await?
            .into_iter()
            .filter_map(|dataset| {
                let haystack = format!(
                    "{} {}",
                    dataset.title,
                    dataset.description.as_deref().unwrap_or_default()
                );
                matcher
                    .fuzzy_match(&haystack, query)
                    .map(|score| (score, dataset))
            })
            .collect::<Vec<_>>();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(scored.into_iter().take(limit).map(|(_, d)| d).collect())
    }
}
