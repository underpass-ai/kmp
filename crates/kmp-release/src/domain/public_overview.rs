use crate::domain::release_error::ReleaseError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicOverview(String);

impl PublicOverview {
    const BEGIN: &'static str = "<!-- kmp:public-overview:begin -->";
    const END: &'static str = "<!-- kmp:public-overview:end -->";

    pub fn parse(document: &str) -> Result<Self, ReleaseError> {
        let (start, end) = Self::bounds(document)?;
        Ok(Self(document[start..end].to_string()))
    }

    pub fn render_into(&self, document: &str) -> Result<String, ReleaseError> {
        let (start, end) = Self::bounds(document)?;
        let mut rendered = document.to_string();
        rendered.replace_range(start..end, &self.0);
        Ok(rendered)
    }

    fn bounds(document: &str) -> Result<(usize, usize), ReleaseError> {
        if document.matches(Self::BEGIN).count() != 1 || document.matches(Self::END).count() != 1 {
            return Err(ReleaseError::invalid(
                "README must contain exactly one public-overview marker pair",
            ));
        }
        let start = document.find(Self::BEGIN).expect("marker count checked");
        let end = document[start..]
            .find(Self::END)
            .map(|offset| start + offset + Self::END.len())
            .ok_or_else(|| ReleaseError::invalid("public-overview markers are reversed"))?;
        Ok((start, end))
    }
}
