//! CLI profile parsing stays outside the transport-independent analysis model.

use crate::batch::BatchProfile;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Profile {
    Syntax,
    Semantic,
}

impl From<Profile> for BatchProfile {
    /// Both tools select the same rule profile without making core depend on clap.
    fn from(profile: Profile) -> Self {
        match profile {
            Profile::Syntax => Self::Syntax,
            Profile::Semantic => Self::Semantic,
        }
    }
}
