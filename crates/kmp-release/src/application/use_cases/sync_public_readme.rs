use std::path::{Path, PathBuf};

use crate::domain::release_error::ReleaseError;
use crate::ports::release_file_system::ReleaseFileSystem;

const BEGIN: &str = "<!-- kmp:public-overview:begin -->";
const END: &str = "<!-- kmp:public-overview:end -->";

pub fn sync_public_readme(
    file_system: &impl ReleaseFileSystem,
    source: &Path,
    targets: &[PathBuf],
) -> Result<usize, ReleaseError> {
    let source_text = file_system.read_text(source)?;
    let (start, end) = bounds(&source_text)?;
    let overview = &source_text[start..end];
    let mut changed = 0;
    for target in targets {
        let current = file_system.read_text(target)?;
        let (start, end) = bounds(&current)?;
        let mut expected = current.clone();
        expected.replace_range(start..end, overview);
        if current != expected {
            file_system.write_text(target, &expected)?;
            changed += 1;
        }
    }
    Ok(changed)
}

fn bounds(document: &str) -> Result<(usize, usize), ReleaseError> {
    if document.matches(BEGIN).count() != 1 || document.matches(END).count() != 1 {
        return Err(ReleaseError::invalid(
            "README must contain exactly one public-overview marker pair",
        ));
    }
    let start = document.find(BEGIN).expect("marker count checked");
    let end = document[start..]
        .find(END)
        .map(|offset| start + offset + END.len())
        .ok_or_else(|| ReleaseError::invalid("public-overview markers are reversed"))?;
    Ok((start, end))
}
