use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::cassette_judgement::CassetteJudgement;
use super::cassette_mode::CassetteMode;
use super::typesafe_judgement::TypeSafeJudgement;
use crate::serving::ports::judgement_model::JudgementModel;

/// The store's judgement model: TypeSafe over the network, or, for
/// evaluation, a cassette of recorded answers in front of it. Replay needs
/// the store's opt-in for the model name but no key and no network.
pub(super) fn load_judgement(
    data_dir: &Path,
    key: Option<String>,
    cassette: Option<String>,
    mode: Option<String>,
) -> Result<Option<Arc<dyn JudgementModel>>, String> {
    let Some(cassette) = cassette.map(PathBuf::from) else {
        return TypeSafeJudgement::load(data_dir, key);
    };
    match CassetteMode::read(mode.as_deref())? {
        CassetteMode::Replay => match TypeSafeJudgement::configured_model(data_dir)? {
            Some(model) => Ok(Some(Arc::new(CassetteJudgement::replay(&cassette, model)?))),
            None => Ok(None),
        },
        CassetteMode::Record => match TypeSafeJudgement::load(data_dir, key)? {
            Some(inner) => Ok(Some(Arc::new(CassetteJudgement::record(&cassette, inner)?))),
            None => Ok(None),
        },
    }
}
