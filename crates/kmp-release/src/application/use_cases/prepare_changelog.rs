use std::path::Path;

use crate::application::use_cases::check_changelog::CheckChangelog;
use crate::domain::calendar_date::CalendarDate;
use crate::domain::release_error::ReleaseError;
use crate::domain::release_version::ReleaseVersion;
use crate::ports::release_file_system::ReleaseFileSystem;

pub struct PrepareChangelog<'a, F> {
    file_system: &'a F,
}

impl<'a, F: ReleaseFileSystem> PrepareChangelog<'a, F> {
    pub fn new(file_system: &'a F) -> Self {
        Self { file_system }
    }

    pub fn execute(
        &self,
        path: &Path,
        version: &ReleaseVersion,
        date: &CalendarDate,
    ) -> Result<bool, ReleaseError> {
        let text = self.file_system.read_text(path)?;
        let checker = CheckChangelog::new(self.file_system);
        if text.lines().any(|line| {
            line == format!("## [{version}]") || line.starts_with(&format!("## [{version}] - "))
        }) {
            checker.execute(path, version)?;
            return Ok(false);
        }
        let unreleased_heading = "## [Unreleased]";
        let start = text.find(unreleased_heading).ok_or_else(|| {
            ReleaseError::invalid(format!("{}: missing {unreleased_heading}", path.display()))
        })?;
        let body_start = start + unreleased_heading.len();
        let next_relative = text[body_start..].find("\n## [").ok_or_else(|| {
            ReleaseError::invalid(format!(
                "{}: cannot determine the previous release",
                path.display()
            ))
        })?;
        let next = body_start + next_relative + 1;
        let notes = text[body_start..next].trim();
        if !notes
            .lines()
            .any(|line| line.starts_with("- ") && line.len() > 2)
        {
            return Err(ReleaseError::invalid(format!(
                "{}: [Unreleased] is empty and no [{version}] section exists",
                path.display()
            )));
        }
        let previous = text[next..]
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("## ["))
            .and_then(|line| line.split(']').next())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                ReleaseError::invalid(format!(
                    "{}: cannot determine the previous release",
                    path.display()
                ))
            })?;
        let replacement = format!("## [Unreleased]\n\n## [{version}] - {date}\n\n{notes}\n\n");
        let mut updated = format!("{}{}{}", &text[..start], replacement, &text[next..]);
        let old_link_start = updated
            .lines()
            .scan(0usize, |offset, line| {
                let current = *offset;
                *offset += line.len() + 1;
                Some((current, line))
            })
            .find_map(|(offset, line)| line.starts_with("[Unreleased]:").then_some(offset))
            .ok_or_else(|| {
                ReleaseError::invalid(format!(
                    "{}: missing [Unreleased] comparison link",
                    path.display()
                ))
            })?;
        let old_link_end = updated[old_link_start..]
            .find('\n')
            .map_or(updated.len(), |offset| old_link_start + offset);
        let links = format!(
            "[Unreleased]: https://github.com/underpass-ai/kmp/compare/{}...HEAD\n[{}]: https://github.com/underpass-ai/kmp/compare/v{}...{}",
            version.tag(),
            version,
            previous,
            version.tag()
        );
        updated.replace_range(old_link_start..old_link_end, &links);
        self.file_system.write_text(path, &updated)?;
        checker.execute(path, version)?;
        Ok(true)
    }
}
