use crate::model::Presentation;
use anyhow::Result;

pub fn parse(input: &str) -> Result<Presentation> {
    let presentation: Presentation = serde_yaml::from_str(input)?;
    Ok(presentation)
}
