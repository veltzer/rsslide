use crate::model::Presentation;
use anyhow::Result;
use std::path::Path;

pub fn export(_presentation: &Presentation, _output_path: &Path) -> Result<()> {
    // TODO: implement PPTX export
    anyhow::bail!("PPTX export not yet implemented")
}
