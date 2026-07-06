//! TIA configuration options.

#[derive(Debug, Clone)]
pub struct TiaOptions {
    pub depth: u32,
    pub filter: Option<globset::GlobSet>,
    pub include_test_files: bool,
    pub transitive: bool,
}

impl Default for TiaOptions {
    fn default() -> Self {
        Self {
            depth: 5,
            filter: None,
            include_test_files: true,
            transitive: false,
        }
    }
}

impl TiaOptions {
    pub fn with_depth(mut self, depth: u32) -> Self {
        self.depth = depth;
        self
    }

    pub fn with_filter_pattern(pattern: &str) -> Result<Self, globset::Error> {
        let mut builder = globset::GlobSetBuilder::new();
        builder.add(globset::Glob::new(pattern)?);
        Ok(Self {
            filter: Some(builder.build()?),
            ..Default::default()
        })
    }
}

pub fn is_test_path(path: &str, filter: &Option<globset::GlobSet>) -> bool {
    if let Some(glob) = filter {
        return glob.is_match(path);
    }
    ax_graph::query_utils::is_test_file(path)
}
